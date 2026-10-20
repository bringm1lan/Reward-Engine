//! Permissionless fee/allocation/refill keeper. X/content decisions belong to
//! the existing backend verifier, whose key is deliberately not needed here.
use anchor_lang::system_program;
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use base64::{engine::general_purpose::STANDARD, Engine};
use distribution_router::{
    accounts, instruction, Campaign, DistributionVault, PurchaseOrder, RouterConfig, TokenBank,
    ASSOCIATED, PUMP, PUMP_AMM, PUMP_FEES, ROUTER_SEED, TOKEN, VAULT_SEED, WSOL,
};
use serde::Deserialize;
use serde_json::{json, Value};
use solana_sdk::{
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    transaction::Transaction,
};
use std::{error::Error, fs, str::FromStr, thread, time::Duration};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    rpc_url: String,
    program_id: String,
    #[serde(default = "interval")]
    interval_seconds: u64,
    projects: Vec<Project>,
}
fn interval() -> u64 {
    60
}
fn threshold() -> u64 {
    100_000
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    mint: String,
    #[serde(default)]
    campaigns: Vec<String>,
    #[serde(default = "threshold")]
    minimum_fee_lamports: u64,
    /// Explicit opt-in inventory settlement, never an arbitrary DEX transaction.
    #[serde(default)]
    purchase_source_token_account: Option<String>,
    #[serde(default)]
    purchases: Vec<ApprovedPurchase>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovedPurchase {
    id: String,
    maximum_tokens: u64,
    minimum_sol_lamports: u64,
}
struct Account {
    lamports: u64,
    owner: Pubkey,
    data: Vec<u8>,
}
struct Rpc {
    url: String,
    agent: ureq::Agent,
}
impl Rpc {
    fn call(&self, method: &str, params: Value) -> Result<Value> {
        let response = self
            .agent
            .post(&self.url)
            .send_json(json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .map_err(|_| "RPC transport failed")?;
        let response: Value = response.into_json()?;
        if let Some(error) = response.get("error") {
            return Err(format!("RPC {method}: {error}").into());
        }
        Ok(response["result"].clone())
    }
    fn account(&self, address: Pubkey) -> Result<Option<Account>> {
        let result = self.call(
            "getAccountInfo",
            json!([address.to_string(),{"encoding":"base64","commitment":"finalized"}]),
        )?;
        let v = &result["value"];
        if v.is_null() {
            return Ok(None);
        }
        Ok(Some(Account {
            lamports: v["lamports"].as_u64().ok_or("Missing balance")?,
            owner: Pubkey::from_str(v["owner"].as_str().ok_or("Missing owner")?)?,
            data: STANDARD.decode(v["data"][0].as_str().ok_or("Missing account bytes")?)?,
        }))
    }
    fn rent(&self, length: usize) -> Result<u64> {
        self.call("getMinimumBalanceForRentExemption", json!([length]))?
            .as_u64()
            .ok_or_else(|| "Missing rent".into())
    }
    fn send(&self, instruction: Instruction, wallet: &Keypair) -> Result<String> {
        let block = self.call("getLatestBlockhash", json!([{"commitment":"finalized"}]))?;
        let hash = Hash::from_str(
            block["value"]["blockhash"]
                .as_str()
                .ok_or("Missing blockhash")?,
        )?;
        let mut compute_data = vec![2];
        compute_data.extend_from_slice(&600_000u32.to_le_bytes());
        let compute = Instruction {
            program_id: Pubkey::from_str("ComputeBudget111111111111111111111111111111")?,
            accounts: vec![],
            data: compute_data,
        };
        let mut tx = Transaction::new_with_payer(&[compute, instruction], Some(&wallet.pubkey()));
        tx.try_sign(&[wallet], hash)?;
        let encoded = STANDARD.encode(bincode::serialize(&tx)?);
        let signature=self.call("sendTransaction",json!([encoded,{"encoding":"base64","skipPreflight":false,"preflightCommitment":"finalized"}]))?.as_str().ok_or("Missing signature")?.to_owned();
        for _ in 0..90 {
            let s = self.call("getSignatureStatuses", json!([[signature]]))?;
            let status = &s["value"][0];
            if !status["err"].is_null() {
                return Err(format!("Transaction failed: {}", status["err"]).into());
            }
            if status["confirmationStatus"] == "finalized" {
                println!("finalized {signature}");
                return Ok(signature);
            }
            thread::sleep(Duration::from_secs(1));
        }
        Err("Confirmation timed out; receipt/sequence guards permit a later tick to reconcile safely".into())
    }
}
fn pda(program: Pubkey, seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &program).0
}
fn ata(owner: Pubkey) -> Pubkey {
    pda(ASSOCIATED, &[owner.as_ref(), TOKEN.as_ref(), WSOL.as_ref()])
}
fn event(program: Pubkey) -> Pubkey {
    pda(program, &[b"__event_authority"])
}
fn ix(
    program_id: Pubkey,
    accounts: impl ToAccountMetas,
    data: impl InstructionData,
) -> Instruction {
    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}
fn append(
    mut ix: Instruction,
    addresses: &[Pubkey],
    writable: &[usize],
    signers: &[usize],
) -> Instruction {
    ix.accounts
        .extend(addresses.iter().enumerate().map(|(i, key)| {
            if writable.contains(&i) {
                AccountMeta::new(*key, signers.contains(&i))
            } else {
                AccountMeta::new_readonly(*key, signers.contains(&i))
            }
        }));
    ix
}
fn u64_at(data: &[u8], offset: usize) -> Option<u64> {
    data.get(offset..offset + 8)
        .map(|n| u64::from_le_bytes(n.try_into().expect("eight bytes")))
}
fn decode<T: AccountDeserialize>(account: Account, program: Pubkey) -> Result<T> {
    if account.owner != program {
        return Err("Account has the wrong program owner".into());
    }
    Ok(T::try_deserialize(&mut account.data.as_slice())?)
}
fn shares(data: &[u8]) -> Result<Vec<Pubkey>> {
    if data.len() < 80 {
        return Err("Truncated Pump shares".into());
    }
    let n = u32::from_le_bytes(data[76..80].try_into()?) as usize;
    if n == 0 || n > 10 || data.len() < 80 + n * 34 {
        return Err("Invalid Pump shares".into());
    }
    (0..n)
        .map(|i| {
            Ok(Pubkey::new_from_array(
                data[80 + i * 34..112 + i * 34].try_into()?,
            ))
        })
        .collect()
}
fn collect(
    rpc: &Rpc,
    program: Pubkey,
    vault_key: Pubkey,
    v: &DistributionVault,
    payer: &Keypair,
    minimum: u64,
) -> Result<()> {
    if v.fee_kind != 1 {
        return Ok(());
    }
    let router = pda(program, &[ROUTER_SEED]);
    let curve = pda(PUMP, &[b"bonding-curve", v.mint.as_ref()]);
    let config = v.fee_source;
    let cv = pda(PUMP, &[b"creator-vault", config.as_ref()]);
    let av = pda(PUMP_AMM, &[b"creator_vault", config.as_ref()]);
    let curve_account = rpc.account(curve)?.ok_or("Pump curve missing")?;
    // Published native-quote creator-fee bucket offset, see docs/ABI.md. Older
    // layouts have no bucket counter and are handled through the legacy vault.
    if u64_at(&curve_account.data, 125).unwrap_or(0) >= minimum {
        let a = [
            payer.pubkey(),
            pda(PUMP, &[b"global"]),
            v.mint,
            WSOL,
            TOKEN,
            ASSOCIATED,
            system_program::ID,
            curve,
            ata(curve),
            cv,
            ata(cv),
            event(PUMP),
            PUMP,
        ];
        rpc.send(
            append(
                ix(
                    program,
                    accounts::SweepPumpBuckets {
                        payer: payer.pubkey(),
                        vault: vault_key,
                        curve,
                        external_program: PUMP,
                    },
                    instruction::SweepPumpBuckets { amm: false },
                ),
                &a,
                &[0, 7, 8, 9, 10],
                &[0],
            ),
            payer,
        )?;
    }
    if curve_account.data.get(48) == Some(&1) {
        let authority = pda(PUMP, &[b"pool-authority", v.mint.as_ref()]);
        let pool = pda(
            PUMP_AMM,
            &[
                b"pool",
                &0u16.to_le_bytes(),
                authority.as_ref(),
                v.mint.as_ref(),
                WSOL.as_ref(),
            ],
        );
        if let Some(pool_account) = rpc.account(pool)? {
            if u64_at(&pool_account.data, 279).unwrap_or(0) >= minimum {
                let a = [
                    payer.pubkey(),
                    pda(PUMP_AMM, &[b"global_config"]),
                    pool,
                    WSOL,
                    TOKEN,
                    ata(pool),
                    av,
                    ata(av),
                    system_program::ID,
                    ASSOCIATED,
                    event(PUMP_AMM),
                    PUMP_AMM,
                ];
                rpc.send(
                    append(
                        ix(
                            program,
                            accounts::SweepPumpBuckets {
                                payer: payer.pubkey(),
                                vault: vault_key,
                                curve,
                                external_program: PUMP_AMM,
                            },
                            instruction::SweepPumpBuckets { amm: true },
                        ),
                        &a,
                        &[0, 2, 5, 7],
                        &[0],
                    ),
                    payer,
                )?;
            }
        }
        if let Some(source) = rpc.account(ata(av))? {
            if u64_at(&source.data, 64).unwrap_or(0) >= minimum {
                let a = [
                    payer.pubkey(),
                    WSOL,
                    TOKEN,
                    system_program::ID,
                    ASSOCIATED,
                    config,
                    av,
                    ata(av),
                    cv,
                    ata(cv),
                    event(PUMP_AMM),
                    PUMP_AMM,
                ];
                rpc.send(
                    append(
                        ix(
                            program,
                            accounts::CollectPump {
                                router,
                                payer: payer.pubkey(),
                                vault: vault_key,
                                external_program: PUMP_AMM,
                            },
                            instruction::SweepPumpAmm {},
                        ),
                        &a,
                        &[0, 6, 7, 8, 9],
                        &[0],
                    ),
                    payer,
                )?;
            }
        }
    }
    if let Some(source) = rpc.account(cv)? {
        if source.lamports.saturating_sub(rpc.rent(0)?) >= minimum {
            let sharing = rpc.account(config)?.ok_or("Pump sharing config missing")?;
            if sharing.owner != PUMP_FEES {
                return Err("Wrong Pump sharing owner".into());
            }
            let shareholders = shares(&sharing.data)?;
            let mut a = vec![
                payer.pubkey(),
                v.mint,
                curve,
                config,
                cv,
                system_program::ID,
                event(PUMP),
                PUMP,
                ata(cv),
                WSOL,
                TOKEN,
                ASSOCIATED,
            ];
            a.extend(shareholders);
            let mut writable = vec![0, 4, 8];
            writable.extend(12..a.len());
            rpc.send(
                append(
                    ix(
                        program,
                        accounts::CollectPump {
                            router,
                            payer: payer.pubkey(),
                            vault: vault_key,
                            external_program: PUMP,
                        },
                        instruction::CollectPumpFees {
                            minimum_received: 1,
                        },
                    ),
                    &a,
                    &writable,
                    &[0],
                ),
                payer,
            )?;
        }
    }
    Ok(())
}
fn campaign_id(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.is_ascii() {
        return Err("Campaign IDs must be 64 hex characters".into());
    }
    let mut out = [0u8; 32];
    for (i, n) in out.iter_mut().enumerate() {
        *n = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}
fn chain_time(rpc: &Rpc) -> Result<i64> {
    let clock = rpc
        .account(solana_sdk::sysvar::clock::ID)?
        .ok_or("Clock missing")?;
    Ok(i64::from_le_bytes(
        clock
            .data
            .get(32..40)
            .ok_or("Truncated clock")?
            .try_into()?,
    ))
}
fn approved_fill(order: &PurchaseOrder, approval: &ApprovedPurchase) -> Result<u64> {
    if approval.minimum_sol_lamports == 0
        || approval.maximum_tokens == 0
        || order.minimum_tokens == 0
        || order.minimum_tokens > approval.maximum_tokens
        || order.input_lamports < approval.minimum_sol_lamports
    {
        return Err("Purchase exceeds the solver's explicit price/amount approval".into());
    }
    Ok(order.minimum_tokens)
}
fn settle_purchases(
    rpc: &Rpc,
    program: Pubkey,
    vault: Pubkey,
    project: &Project,
    wallet: Option<&Keypair>,
) -> Result<()> {
    for approval in &project.purchases {
        let id = campaign_id(&approval.id)?;
        let key = pda(program, &[b"purchase", vault.as_ref(), &id]);
        let order: PurchaseOrder = decode(rpc.account(key)?.ok_or("Purchase missing")?, program)?;
        if order.vault != vault || order.id != id {
            return Err("Purchase belongs to a different vault or ID".into());
        }
        if order.status != 0 {
            continue;
        }
        println!(
            "purchase {}: {} lamports for at least {} token base units",
            approval.id, order.input_lamports, order.minimum_tokens
        );
        let Some(wallet) = wallet else { continue };
        if chain_time(rpc)? >= order.expires_at {
            rpc.send(
                ix(
                    program,
                    accounts::CancelPurchase {
                        actor: wallet.pubkey(),
                        vault,
                        order: key,
                    },
                    instruction::CancelPurchase {},
                ),
                wallet,
            )?;
            continue;
        }
        let Some(source) = &project.purchase_source_token_account else {
            continue;
        };
        let amount = approved_fill(&order, approval)?;
        let source = Pubkey::from_str(source)?;
        let info = rpc.account(source)?.ok_or("Solver token account missing")?;
        if info.owner != TOKEN
            || info.data.get(..32) != Some(order.reward_mint.as_ref())
            || info.data.get(32..64) != Some(wallet.pubkey().as_ref())
        {
            return Err("Solver source must be a legacy SPL account owned by the keeper for the approved mint".into());
        }
        if u64_at(&info.data, 64).ok_or("Truncated solver token account")? < amount {
            println!("purchase {}: waiting for solver inventory", approval.id);
            continue;
        }
        let bank = pda(
            program,
            &[b"token_bank", vault.as_ref(), order.reward_mint.as_ref()],
        );
        rpc.send(
            ix(
                program,
                accounts::FillPurchase {
                    router: pda(program, &[ROUTER_SEED]),
                    executor: wallet.pubkey(),
                    vault,
                    order: key,
                    mint: order.reward_mint,
                    bank,
                    source,
                    bank_tokens: pda(program, &[b"bank_tokens", bank.as_ref()]),
                    token_program: TOKEN,
                },
                instruction::FillPurchase {
                    token_amount: amount,
                },
            ),
            wallet,
        )?;
    }
    Ok(())
}
fn tick(rpc: &Rpc, program: Pubkey, project: &Project, wallet: Option<&Keypair>) -> Result<()> {
    let mint = Pubkey::from_str(&project.mint)?;
    let vault_key = pda(program, &[VAULT_SEED, mint.as_ref()]);
    let router = pda(program, &[ROUTER_SEED]);
    let config: RouterConfig = decode(rpc.account(router)?.ok_or("Router missing")?, program)?;
    let wallet = if config.paused { None } else { wallet };
    let info = rpc
        .account(vault_key)?
        .ok_or("Project vault does not exist")?;
    let mut v: DistributionVault = decode(info, program)?;
    if let Some(wallet) = wallet {
        if !v.paused {
            collect(
                rpc,
                program,
                vault_key,
                &v,
                wallet,
                project.minimum_fee_lamports.max(1),
            )?;
        }
    }
    let info = rpc.account(vault_key)?.ok_or("Vault disappeared")?;
    let balance = info.lamports;
    let length = info.data.len();
    v = decode(info, program)?;
    let allocated = v
        .balances
        .iter()
        .try_fold(0u64, |sum, n| sum.checked_add(*n).ok_or("Counter overflow"))?;
    let available = balance
        .checked_sub(rpc.rent(length)?)
        .and_then(|n| n.checked_sub(allocated))
        .ok_or("Vault accounting mismatch")?;
    println!(
        "vault {mint}: {available} unallocated lamports; buckets {:?}",
        v.balances
    );
    if let Some(wallet) = wallet {
        if !v.paused && available >= v.policy.minimum_route_lamports && available > 0 {
            rpc.send(
                ix(
                    program,
                    accounts::Allocate {
                        router,
                        vault: vault_key,
                    },
                    instruction::Route {
                        sequence: v.route_sequence,
                        version: v.rules_version,
                        minimum: v.policy.minimum_route_lamports,
                    },
                ),
                wallet,
            )?;
        }
    }
    settle_purchases(
        rpc,
        program,
        vault_key,
        project,
        if v.paused { None } else { wallet },
    )?;
    for id in &project.campaigns {
        let id = campaign_id(id)?;
        let campaign_key = pda(program, &[b"campaign", vault_key.as_ref(), &id]);
        let mut campaign: Campaign = decode(
            rpc.account(campaign_key)?.ok_or("Campaign missing")?,
            program,
        )?;
        if campaign.status != 0 {
            continue;
        }
        let now = chain_time(rpc)?;
        if now >= campaign.rules.ends_at && !campaign.rules.recurring {
            if let Some(wallet) = wallet {
                if campaign.rules.reward_mint == Pubkey::default() {
                    rpc.send(
                        ix(
                            program,
                            accounts::CloseCampaignSol {
                                actor: wallet.pubkey(),
                                vault: vault_key,
                                campaign: campaign_key,
                            },
                            instruction::CloseCampaignSol {},
                        ),
                        wallet,
                    )?;
                } else {
                    let mint = campaign.rules.reward_mint;
                    let bank = pda(program, &[b"token_bank", vault_key.as_ref(), mint.as_ref()]);
                    rpc.send(
                        ix(
                            program,
                            accounts::CloseCampaignTokens {
                                actor: wallet.pubkey(),
                                vault: vault_key,
                                campaign: campaign_key,
                                mint,
                                bank,
                                bank_tokens: pda(program, &[b"bank_tokens", bank.as_ref()]),
                                escrow: pda(program, &[b"campaign_tokens", campaign_key.as_ref()]),
                                token_program: TOKEN,
                            },
                            instruction::CloseCampaignTokens {},
                        ),
                        wallet,
                    )?;
                }
            }
            continue;
        }
        if now >= campaign.rules.ends_at && campaign.rules.recurring {
            if let Some(wallet) = wallet {
                rpc.send(
                    ix(
                        program,
                        accounts::AdvanceEpoch {
                            router,
                            vault: vault_key,
                            campaign: campaign_key,
                        },
                        instruction::AdvanceCampaignEpoch {},
                    ),
                    wallet,
                )?;
                campaign = decode(
                    rpc.account(campaign_key)?.ok_or("Campaign missing")?,
                    program,
                )?;
            }
        }
        if !campaign.rules.auto_refill
            || now < campaign.rules.starts_at
            || now >= campaign.rules.ends_at
            || campaign.remaining > campaign.rules.refill_threshold
            || campaign.epoch_funded >= campaign.rules.max_epoch_budget
        {
            continue;
        }
        if let Some(wallet) = wallet {
            if campaign.rules.reward_mint == Pubkey::default() {
                let current: DistributionVault =
                    decode(rpc.account(vault_key)?.ok_or("Vault missing")?, program)?;
                if current.balances[campaign.rules.bucket as usize] > 0 {
                    rpc.send(
                        ix(
                            program,
                            accounts::FundCampaignSol {
                                actor: wallet.pubkey(),
                                router,
                                vault: vault_key,
                                campaign: campaign_key,
                            },
                            instruction::RefillCampaignSol {},
                        ),
                        wallet,
                    )?;
                }
            } else {
                let mint = campaign.rules.reward_mint;
                let bank_key = pda(program, &[b"token_bank", vault_key.as_ref(), mint.as_ref()]);
                if let Some(info) = rpc.account(bank_key)? {
                    let bank: TokenBank = decode(info, program)?;
                    if bank.balances[campaign.rules.bucket as usize] > 0 {
                        rpc.send(
                            ix(
                                program,
                                accounts::FundCampaignTokens {
                                    actor: wallet.pubkey(),
                                    router,
                                    vault: vault_key,
                                    campaign: campaign_key,
                                    mint,
                                    bank: bank_key,
                                    bank_tokens: pda(program, &[b"bank_tokens", bank_key.as_ref()]),
                                    escrow: pda(
                                        program,
                                        &[b"campaign_tokens", campaign_key.as_ref()],
                                    ),
                                    token_program: TOKEN,
                                },
                                instruction::RefillCampaignTokens {},
                            ),
                            wallet,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] == "--help" {
        println!("distribution-keeper inspect <config.json>\ndistribution-keeper tick <config.json> <keeper-wallet.json>\ndistribution-keeper watch <config.json> <keeper-wallet.json>\nInspect is read-only. Tick/watch collect, allocate, refill and optionally settle explicitly approved inventory RFQs.");
        return Ok(());
    }
    let command = &args[1];
    if !["inspect", "tick", "watch"].contains(&command.as_str()) {
        return Err("Unknown command".into());
    }
    let config: Config = serde_json::from_slice(&fs::read(&args[2])?)?;
    let program = Pubkey::from_str(&config.program_id)?;
    let wallet = if command == "inspect" {
        None
    } else {
        Some(
            read_keypair_file(
                args.get(3)
                    .ok_or("A dedicated keeper wallet path is required")?,
            )
            .map_err(|_| "Cannot load keeper wallet")?,
        )
    };
    let rpc = Rpc {
        url: config.rpc_url,
        agent: ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build(),
    };
    loop {
        for project in &config.projects {
            if let Err(error) = tick(&rpc, program, project, wallet.as_ref()) {
                eprintln!("Project {}: {error}", project.mint);
                if command != "watch" {
                    return Err(error);
                }
            }
        }
        if command != "watch" {
            break;
        }
        thread::sleep(Duration::from_secs(config.interval_seconds.max(10)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_require_exact_ascii_hex() {
        assert_eq!(campaign_id(&"01".repeat(32)).unwrap(), [1; 32]);
        assert!(campaign_id(&"z0".repeat(32)).is_err());
        assert!(campaign_id(&"é".repeat(32)).is_err());
        assert!(campaign_id("00").is_err());
    }
    #[test]
    fn shares_reject_truncated_vectors() {
        assert!(shares(&[0; 79]).is_err());
        let mut data = vec![0; 114];
        data[76..80].copy_from_slice(&1u32.to_le_bytes());
        data[80..112].fill(7);
        assert_eq!(
            shares(&data).unwrap(),
            vec![Pubkey::new_from_array([7; 32])]
        );
        data.pop();
        assert!(shares(&data).is_err());
    }
    #[test]
    fn solver_cannot_fill_outside_its_independent_approval() {
        let mut bytes = vec![0; 8 + PurchaseOrder::INIT_SPACE];
        use anchor_lang::{Discriminator, Space};
        bytes[..8].copy_from_slice(PurchaseOrder::DISCRIMINATOR);
        let mut order = PurchaseOrder::try_deserialize(&mut bytes.as_slice()).unwrap();
        order.input_lamports = 1000;
        order.minimum_tokens = 50;
        let mut approval = ApprovedPurchase {
            id: "01".repeat(32),
            maximum_tokens: 50,
            minimum_sol_lamports: 1000,
        };
        assert_eq!(approved_fill(&order, &approval).unwrap(), 50);
        approval.maximum_tokens = 49;
        assert!(approved_fill(&order, &approval).is_err());
        approval.maximum_tokens = 50;
        approval.minimum_sol_lamports = 1001;
        assert!(approved_fill(&order, &approval).is_err());
        approval.minimum_sol_lamports = 0;
        assert!(approved_fill(&order, &approval).is_err());
    }
}
