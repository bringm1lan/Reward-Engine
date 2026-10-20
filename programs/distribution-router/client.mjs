// Unsigned Node instruction builders. No wallet files or credentials are loaded.
import { createHash } from 'node:crypto';
import { PublicKey, SystemProgram, TransactionInstruction } from '@solana/web3.js';
export const DEVELOPMENT_PROGRAM_ID = new PublicKey('Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkgMQHGJkY4KT');
export const IDS = Object.fromEntries(Object.entries({
  pump:'6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P', fees:'pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ',
  amm:'pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA', launchlab:'LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj',
  stonk:'AvVCE7Ue49iZjYzkkHz6ZhVyvY6NLHw67vB8eQaffVPz', wsol:'So11111111111111111111111111111111111111112',
  token:'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA', associated:'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL',
}).map(([name,value])=>[name,new PublicKey(value)]));
const LOADER = new PublicKey('BPFLoaderUpgradeab1e11111111111111111111111');
const pk = (v) => new PublicKey(v);
const meta = (pubkey,isSigner=false,isWritable=false) => ({pubkey:pk(pubkey),isSigner,isWritable});
const disc = (namespace,name) => createHash('sha256').update(`${namespace}:${name}`).digest().subarray(0,8);
export const evidenceId = (value) => createHash('sha256').update(String(value)).digest();
const fixed = (value) => { const b=Buffer.from(value); if(b.length!==32)throw new Error('Expected 32-byte evidence ID'); return b; };
const bool = (v) => Buffer.from([v?1:0]);
function integer(value,size,signed=false) {
  if(typeof value==='number'&&!Number.isSafeInteger(value))throw new Error('Use bigint for monetary values');
  const b=Buffer.alloc(size); if(size===8){ if(signed)b.writeBigInt64LE(BigInt(value));else b.writeBigUInt64LE(BigInt(value)); }
  else if(size===4)b.writeUInt32LE(value);else b.writeUInt16LE(value);return b;
}
const u64=(v)=>integer(v,8); const i64=(v)=>integer(v,8,true);const u32=(v)=>integer(v,4);const u16=(v)=>integer(v,2);
const encodePolicy=({shareBps,minimumRouteLamports})=>{
  if(shareBps.length!==5||shareBps.some(n=>!Number.isInteger(n)||n<0||n>10000)||shareBps.reduce((a,b)=>a+b,0)!==10000)throw new Error('Five shares must total 10000 bps');
  return Buffer.concat([...shareBps.map(u16),u64(minimumRouteLamports)]);
};
const encodeRules=(r)=>Buffer.concat([pk(r.verifier).toBuffer(),pk(r.rewardMint??SystemProgram.programId).toBuffer(),Buffer.from([r.bucket??0]),i64(r.startsAt),i64(r.endsAt),u64(r.minReward),u64(r.maxReward),u16(r.rollingLimit??5),u32(r.windowSeconds??3600),u32(r.dailyLimit??60),u32(r.lifetimeLimit??0),u64(r.maxEpochBudget),u64(r.refillAmount??0),u64(r.refillThreshold??0),bool(r.autoRefill),bool(r.recurring),fixed(r.rulesHash)]);
const encodeAward=(a)=>Buffer.concat([fixed(a.postId),fixed(a.userId),u64(a.amount),i64(a.expiresAt),fixed(a.rulesHash)]);
export const derive=(program,...seeds)=>PublicKey.findProgramAddressSync(seeds.map(s=>typeof s==='string'?Buffer.from(s):s instanceof PublicKey?s.toBuffer():Buffer.from(s)),pk(program))[0];
const ata=(owner)=>derive(IDS.associated,pk(owner),IDS.token,IDS.wsol);
const event=(program)=>derive(program,'__event_authority');

export function createRouterClient(programId=DEVELOPMENT_PROGRAM_ID) {
  programId=pk(programId);const router=derive(programId,'distribution_router');
  const vaultFor=(mint)=>derive(programId,'distribution_vault',pk(mint));
  const campaignFor=(mint,id)=>derive(programId,'campaign',vaultFor(mint),fixed(id));
  const bankFor=(mint,rewardMint)=>derive(programId,'token_bank',vaultFor(mint),pk(rewardMint));
  const bankTokensFor=(mint,rewardMint)=>derive(programId,'bank_tokens',bankFor(mint,rewardMint));
  const campaignTokensFor=(mint,id)=>derive(programId,'campaign_tokens',campaignFor(mint,id));
  const orderFor=(mint,id)=>derive(programId,'purchase',vaultFor(mint),fixed(id));
  const ix=(name,keys,...args)=>new TransactionInstruction({programId,keys,data:Buffer.concat([disc('global',name),...args])});
  const manage=(name,{manager,mint},...args)=>ix(name,[meta(manager,true),meta(vaultFor(mint),false,true)],...args);
  const admin=(name,authority,...args)=>ix(name,[meta(authority,true),meta(router,false,true)],...args);
  const campaignKeys=({actor,mint,id})=>[meta(actor,true),meta(router),meta(vaultFor(mint),false,true),meta(campaignFor(mint,id),false,true)];
  const tokenFundingKeys=(p)=>[...campaignKeys(p),meta(p.rewardMint),meta(bankFor(p.mint,p.rewardMint),false,true),meta(bankTokensFor(p.mint,p.rewardMint),false,true),meta(campaignTokensFor(p.mint,p.id),false,true),meta(IDS.token)];
  const awardKeys=(p)=>[meta(p.payer,true,true),meta(p.verifier,true),meta(router),meta(vaultFor(p.mint)),meta(campaignFor(p.mint,p.id),false,true),meta(p.recipient,false,!p.rewardMint),meta(derive(programId,'participant',campaignFor(p.mint,p.id),fixed(p.award.userId)),false,true),meta(derive(programId,'reward',campaignFor(p.mint,p.id),fixed(p.award.postId)),false,true)];
  const curveFor=(mint)=>derive(IDS.pump,'bonding-curve',pk(mint));
  const sharingFor=(mint)=>derive(IDS.fees,'sharing-config',pk(mint));
  const poolFor=(mint)=>derive(IDS.amm,'pool',Buffer.alloc(2),derive(IDS.pump,'pool-authority',pk(mint)),pk(mint),IDS.wsol);
  const feePrefix=(p,program)=>[meta(router),meta(p.manager,true),meta(p.creator,true,true),meta(vaultFor(p.mint),false,true),meta(program)];
  const cpiKeys=(addresses,writable,signers)=>addresses.map((key,index)=>meta(key,signers.includes(index),writable.includes(index)));
  const collectPrefix=(payer,mint,program)=>[meta(router),meta(payer,true,true),meta(vaultFor(mint),false,true),meta(program)];
  return {
    programId,router,vaultFor,campaignFor,bankFor,bankTokensFor,campaignTokensFor,orderFor,curveFor,sharingFor,poolFor,
    initializeRouter:({authority,activationTreasury,rewardMint})=>ix('initialize_router',[meta(authority,true,true),meta(programId),meta(derive(LOADER,programId)),meta(activationTreasury),meta(router,false,true),meta(SystemProgram.programId)],pk(rewardMint).toBuffer()),
    configureRouter:({authority,activationTreasury,activationFeeLamports,paused})=>admin('configure_router',authority,pk(activationTreasury).toBuffer(),u64(activationFeeLamports),bool(paused)),
    registerVault:({authority,manager,mint,activationTreasury,policy})=>ix('register_vault',[meta(authority,true),meta(router),meta(manager,true,true),meta(mint),meta(activationTreasury,false,true),meta(vaultFor(mint),false,true),meta(SystemProgram.programId)],encodePolicy(policy)),
    deposit:({contributor,mint,amount})=>ix('deposit',[meta(contributor,true,true),meta(vaultFor(mint),false,true),meta(SystemProgram.programId)],u64(amount)),
    route:({mint,expectedSequence,expectedRulesVersion,minimumAmount=1n})=>ix('route',[meta(router),meta(vaultFor(mint),false,true)],u64(expectedSequence),u64(expectedRulesVersion),u64(minimumAmount)),
    configureVault:(p)=>manage('configure_vault',p,encodePolicy(p.policy)),
    pauseVault:(p)=>manage('pause_vault',p,bool(p.paused)),
    rebalanceSol:(p)=>manage('rebalance_sol',p,Buffer.from([p.from,p.to]),u64(p.amount)),
    withdrawReserve:(p)=>ix('withdraw_reserve',[meta(router),meta(p.manager,true),meta(vaultFor(p.mint),false,true),meta(p.recipient,false,true)],u64(p.amount)),
    proposeManager:(p)=>manage('propose_manager',p,pk(p.nextManager).toBuffer()),
    acceptManager:(p)=>manage('accept_manager',{manager:p.nextManager,mint:p.mint}),
    proposeAuthority:(p)=>admin('propose_authority',p.authority,pk(p.nextAuthority).toBuffer()),
    acceptAuthority:(p)=>admin('accept_authority',p.nextAuthority),
    initializeTokenBank:(p)=>ix('initialize_token_bank',[meta(p.payer,true,true),meta(vaultFor(p.mint)),meta(p.rewardMint),meta(bankFor(p.mint,p.rewardMint),false,true),meta(bankTokensFor(p.mint,p.rewardMint),false,true),meta(IDS.token),meta(SystemProgram.programId)]),
    depositTokens:(p)=>ix('deposit_tokens',[meta(p.contributor,true),meta(p.rewardMint),meta(bankFor(p.mint,p.rewardMint),false,true),meta(p.source,false,true),meta(bankTokensFor(p.mint,p.rewardMint),false,true),meta(IDS.token)],u64(p.amount),Buffer.from([p.bucket??0])),
    syncTokenDonations:(p)=>ix('sync_token_donations',[meta(p.manager,true),meta(vaultFor(p.mint)),meta(bankFor(p.mint,p.rewardMint),false,true),meta(bankTokensFor(p.mint,p.rewardMint))],Buffer.from([p.bucket??0])),
    createPurchase:(p)=>ix('create_purchase',[meta(router),meta(p.manager,true,true),meta(vaultFor(p.mint),false,true),meta(orderFor(p.mint,p.id),false,true),meta(SystemProgram.programId)],fixed(p.id),u64(p.amount),u64(p.minimumTokens),i64(p.expiresAt),Buffer.from([p.bucket??0])),
    fillPurchase:(p)=>ix('fill_purchase',[meta(router),meta(p.executor,true,true),meta(vaultFor(p.mint),false,true),meta(orderFor(p.mint,p.id),false,true),meta(p.rewardMint),meta(bankFor(p.mint,p.rewardMint),false,true),meta(p.source,false,true),meta(bankTokensFor(p.mint,p.rewardMint),false,true),meta(IDS.token)],u64(p.tokenAmount)),
    cancelPurchase:(p)=>ix('cancel_purchase',[meta(p.actor,true),meta(vaultFor(p.mint),false,true),meta(orderFor(p.mint,p.id),false,true)]),
    createCampaign:(p)=>ix('create_campaign',[meta(router),meta(p.manager,true,true),meta(vaultFor(p.mint)),meta(campaignFor(p.mint,p.id),false,true),meta(SystemProgram.programId)],fixed(p.id),encodeRules(p.rules)),
    initializeCampaignTokens:(p)=>ix('initialize_campaign_tokens',[meta(p.payer,true,true),meta(campaignFor(p.mint,p.id)),meta(p.rewardMint),meta(campaignTokensFor(p.mint,p.id),false,true),meta(IDS.token),meta(SystemProgram.programId)]),
    fundCampaignSol:(p)=>ix('fund_campaign_sol',campaignKeys(p),u64(p.amount)),
    refillCampaignSol:(p)=>ix('refill_campaign_sol',campaignKeys(p)),
    fundCampaignTokens:(p)=>ix('fund_campaign_tokens',tokenFundingKeys(p),u64(p.amount)),
    refillCampaignTokens:(p)=>ix('refill_campaign_tokens',tokenFundingKeys(p)),
    pauseCampaign:(p)=>ix('pause_campaign',[meta(p.manager,true),meta(vaultFor(p.mint)),meta(campaignFor(p.mint,p.id),false,true)],bool(p.paused)),
    advanceCampaignEpoch:(p)=>ix('advance_campaign_epoch',[meta(router),meta(vaultFor(p.mint)),meta(campaignFor(p.mint,p.id),false,true)]),
    awardSol:(p)=>ix('award_sol',[...awardKeys(p),meta(SystemProgram.programId)],encodeAward(p.award)),
    awardTokens:(p)=>ix('award_tokens',[...awardKeys(p),meta(p.rewardMint),meta(campaignTokensFor(p.mint,p.id),false,true),meta(p.recipientTokens,false,true),meta(IDS.token),meta(SystemProgram.programId)],encodeAward(p.award)),
    closeCampaignSol:(p)=>ix('close_campaign_sol',[meta(p.actor,true),meta(vaultFor(p.mint),false,true),meta(campaignFor(p.mint,p.id),false,true)]),
    closeCampaignTokens:(p)=>ix('close_campaign_tokens',[meta(p.actor,true),meta(vaultFor(p.mint)),meta(campaignFor(p.mint,p.id),false,true),meta(p.rewardMint),meta(bankFor(p.mint,p.rewardMint),false,true),meta(bankTokensFor(p.mint,p.rewardMint),false,true),meta(campaignTokensFor(p.mint,p.id),false,true),meta(IDS.token)]),
    createPumpSharing:(p)=>{
      const a=[event(IDS.fees),IDS.fees,p.creator,derive(IDS.pump,'global'),p.mint,sharingFor(p.mint),SystemProgram.programId,curveFor(p.mint),IDS.pump,event(IDS.pump),p.graduated?poolFor(p.mint):IDS.fees,p.graduated?IDS.amm:IDS.fees,p.graduated?event(IDS.amm):IDS.fees];
      return ix('create_pump_sharing',[...feePrefix(p,IDS.fees),...cpiKeys(a,[2,5,7,...(p.graduated?[10]:[])],[2])]);
    },
    finalizePumpSharing:(p)=>{
      const config=sharingFor(p.mint),cv=derive(IDS.pump,'creator-vault',config),av=derive(IDS.amm,'creator_vault',config);
      const a=[event(IDS.fees),IDS.fees,p.creator,derive(IDS.pump,'global'),p.mint,config,curveFor(p.mint),cv,ata(cv),SystemProgram.programId,IDS.pump,event(IDS.pump),IDS.amm,event(IDS.amm),IDS.wsol,IDS.token,IDS.associated,av,ata(av),p.creator];
      return ix('finalize_pump_sharing',[...feePrefix(p,IDS.fees),...cpiKeys(a,[2,5,7,8,17,18,19],[2])],u16(p.shareBps));
    },
    connectPump:(p)=>ix('connect_pump',[meta(router),meta(p.manager,true),meta(vaultFor(p.mint),false,true),meta(curveFor(p.mint)),meta(sharingFor(p.mint))]),
    sweepPumpAmm:(p)=>{const config=sharingFor(p.mint),cv=derive(IDS.pump,'creator-vault',config),av=derive(IDS.amm,'creator_vault',config);
      const a=[p.payer,IDS.wsol,IDS.token,SystemProgram.programId,IDS.associated,config,av,ata(av),cv,ata(cv),event(IDS.amm),IDS.amm];
      return ix('sweep_pump_amm',[...collectPrefix(p.payer,p.mint,IDS.amm),...cpiKeys(a,[0,6,7,8,9],[0])]);},
    collectPumpFees:(p)=>{const config=sharingFor(p.mint),cv=derive(IDS.pump,'creator-vault',config);
      const a=[p.payer,p.mint,curveFor(p.mint),config,cv,SystemProgram.programId,event(IDS.pump),IDS.pump,ata(cv),IDS.wsol,IDS.token,IDS.associated,...p.shareholders];
      return ix('collect_pump_fees',[...collectPrefix(p.payer,p.mint,IDS.pump),...cpiKeys(a,[0,4,8,...p.shareholders.map((_,i)=>12+i)],[0])],u64(p.minimumReceived??1n));},
    sweepPumpBuckets:(p)=>{const creator=pk(p.curveCreator),curve=curveFor(p.mint),pool=poolFor(p.mint),program=p.amm?IDS.amm:IDS.pump;
      const recipient=derive(program,p.amm?'creator_vault':'creator-vault',creator);
      const a=p.amm?[p.payer,derive(IDS.amm,'global_config'),pool,IDS.wsol,IDS.token,ata(pool),recipient,ata(recipient),SystemProgram.programId,IDS.associated,event(IDS.amm),IDS.amm]:[p.payer,derive(IDS.pump,'global'),p.mint,IDS.wsol,IDS.token,IDS.associated,SystemProgram.programId,curve,ata(curve),recipient,ata(recipient),event(IDS.pump),IDS.pump];
      return ix('sweep_pump_buckets',[meta(p.payer,true,true),meta(vaultFor(p.mint)),meta(curve),meta(program),...cpiKeys(a,p.amm?[0,2,5,7]:[0,7,8,9,10],[0])],bool(p.amm));},
    connectLaunchlab:(p)=>ix('connect_launchlab',[meta(router),meta(p.manager,true),meta(p.creator,true),meta(vaultFor(p.mint),false,true),meta(p.pool),meta(p.platform)],u16(p.shareBps)),
    relayLaunchlabFees:(p)=>ix('relay_launchlab_fees',[meta(router),meta(p.creator,true,true),meta(vaultFor(p.mint),false,true),meta(SystemProgram.programId)],u64(p.amount)),
    claimLaunchlabFees:(p)=>{
      const creator=pk(p.creator),creatorTokens=ata(creator),feeAuthority=derive(IDS.launchlab,'creator_fee_vault_auth_seed'),feeVault=derive(IDS.launchlab,creator,IDS.wsol);
      const a=[creator,feeAuthority,feeVault,creatorTokens,IDS.wsol,IDS.token,SystemProgram.programId,IDS.associated];
      return ix('claim_launchlab_fees',[meta(router),meta(creator,true,true),meta(p.payer,true,true),meta(vaultFor(p.mint),false,true),meta(IDS.wsol),meta(creatorTokens,false,true),meta(derive(programId,'claim_buffer',vaultFor(p.mint)),false,true),meta(IDS.token),meta(SystemProgram.programId),meta(IDS.launchlab),...cpiKeys(a,[0,2,3],[0])],u64(p.minimumReceived??1n));
    },
  };
}

function reader(account,name,programId) {
  if(!account?.owner.equals(pk(programId))||!account.data.subarray(0,8).equals(disc('account',name)))throw new Error(`Invalid ${name} account`);
  let offset=8;const b=account.data; const take=(n)=>{const v=b.subarray(offset,offset+n);if(v.length!==n)throw new Error('Truncated account');offset+=n;return v;};
  return {key:()=>new PublicKey(take(32)),bytes:()=>take(32),u64:()=>take(8).readBigUInt64LE(),i64:()=>take(8).readBigInt64LE(),u32:()=>take(4).readUInt32LE(),u16:()=>take(2).readUInt16LE(),u8:()=>take(1)[0]};
}
export function decodeVault(account,programId=DEVELOPMENT_PROGRAM_ID){const r=reader(account,'DistributionVault',programId);return{mint:r.key(),manager:r.key(),pendingManager:r.key(),policy:{shareBps:Array.from({length:5},r.u16),minimumRouteLamports:r.u64()},balances:Array.from({length:5},r.u64),rulesVersion:r.u64(),routeSequence:r.u64(),totalAllocatedLamports:r.u64(),verifiedCreatorFees:r.u64(),feeKind:r.u8(),feeSource:r.key(),feeCreator:r.key(),feeShareBps:r.u16(),paused:r.u8()!==0,bump:r.u8()};}
export function decodeBank(account,programId=DEVELOPMENT_PROGRAM_ID){const r=reader(account,'TokenBank',programId);return{vault:r.key(),mint:r.key(),balances:Array.from({length:5},r.u64),bump:r.u8()};}
export function decodeCampaign(account,programId=DEVELOPMENT_PROGRAM_ID){const r=reader(account,'Campaign',programId);return{vault:r.key(),id:r.bytes(),rules:{verifier:r.key(),rewardMint:r.key(),bucket:r.u8(),startsAt:r.i64(),endsAt:r.i64(),minReward:r.u64(),maxReward:r.u64(),rollingLimit:r.u16(),windowSeconds:r.u32(),dailyLimit:r.u32(),lifetimeLimit:r.u32(),maxEpochBudget:r.u64(),refillAmount:r.u64(),refillThreshold:r.u64(),autoRefill:r.u8()!==0,recurring:r.u8()!==0,rulesHash:r.bytes()},status:r.u8(),epoch:r.u64(),epochFunded:r.u64(),remaining:r.u64(),totalFunded:r.u64(),totalPaid:r.u64(),rewardCount:r.u64(),bump:r.u8()};}
