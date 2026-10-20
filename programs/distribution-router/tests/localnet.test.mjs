import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { ComputeBudgetProgram, Connection, Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';
import { createRouterClient, decodeVault, decodeBank, decodeCampaign, derive, evidenceId, IDS } from '../client.mjs';

const client=createRouterClient();
const binary=(name)=>fileURLToPath(new URL(`../../../target/deploy/${name}.so`,import.meta.url));
const keeperBinary=fileURLToPath(new URL('../../../keeper/target/debug/distribution-keeper',import.meta.url));
const ZERO=SystemProgram.programId;
async function port(){const server=createServer();await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve)});const n=server.address().port;await new Promise(resolve=>server.close(resolve));return n;}
async function rpcPorts(){for(let i=0;i<10;i++){const p=await port(),s=createServer();try{await new Promise((resolve,reject)=>{s.once('error',reject);s.listen(p+1,'127.0.0.1',resolve)});await new Promise(resolve=>s.close(resolve));return p;}catch{}}throw Error('No available RPC port pair');}
const mintData=(owner)=>{const b=Buffer.alloc(82);b.writeUInt32LE(1);owner.toBuffer().copy(b,4);b.writeBigUInt64LE(1_000_000_000_000n,36);b[44]=9;b[45]=1;return b;};
const tokenData=(mint,owner,amount,native=false)=>{const b=Buffer.alloc(165);mint.toBuffer().copy(b);owner.toBuffer().copy(b,32);b.writeBigUInt64LE(BigInt(amount),64);b[108]=1;if(native){b.writeUInt32LE(1,109);b.writeBigUInt64LE(2_039_280n,113)}return b;};

test('V3 end-to-end local SBF flow (real SPL processor; launchpad CPI fixtures)',{timeout:180_000},async(t)=>{
  for(const name of ['distribution_router','router_test_spl_token','router_test_launchpad_fixture'])await readFile(binary(name));
  await readFile(keeperBinary);
  const directory=await mkdtemp(join(tmpdir(),'ansem-v3-flow-'));
  const authority=Keypair.generate(),manager=Keypair.generate(),outsider=Keypair.generate(),replacement=Keypair.generate();
  const treasury=Keypair.generate().publicKey,recipient=Keypair.generate().publicKey;
  const mint=Keypair.generate().publicKey,stonkMint=Keypair.generate().publicKey,rewardMint=Keypair.generate().publicKey;
  const executorTokens=Keypair.generate().publicKey,recipientTokens=Keypair.generate().publicKey;
  const creatorTokens=derive(IDS.associated,manager.publicKey,IDS.token,IDS.wsol);
  const launchlabFeeAuthority=derive(IDS.launchlab,'creator_fee_vault_auth_seed');
  const launchlabFeeVault=derive(IDS.launchlab,manager.publicKey,IDS.wsol);
  const config=client.sharingFor(mint),curve=client.curveFor(mint),creatorVault=derive(IDS.pump,'creator-vault',config);
  const platform=Keypair.generate().publicKey,launchlabPool=Keypair.generate().publicKey;
  const fixtures=[];
  const account=async(address,owner,data=Buffer.alloc(0),lamports=20_000_000_000)=>{
    const path=join(directory,`account-${fixtures.length}.json`);await writeFile(path,JSON.stringify({pubkey:address.toBase58(),account:{lamports,data:[data.toString('base64'),'base64'],owner:owner.toBase58(),executable:false,rentEpoch:0,space:data.length}}));fixtures.push('--account',address.toBase58(),path);
  };
  for(const key of [authority.publicKey,manager.publicKey,outsider.publicKey,replacement.publicKey,treasury,recipient])await account(key,ZERO);
  for(const key of [mint,stonkMint,rewardMint,IDS.wsol])await account(key,IDS.token,mintData(authority.publicKey),2_000_000);
  await account(executorTokens,IDS.token,tokenData(rewardMint,outsider.publicKey,1_000_000),2_039_280);
  await account(recipientTokens,IDS.token,tokenData(rewardMint,recipient,0),2_039_280);
  await account(creatorTokens,IDS.token,tokenData(IDS.wsol,manager.publicKey,5_000_000,true),7_039_280);
  await account(launchlabFeeVault,IDS.token,tokenData(IDS.wsol,launchlabFeeAuthority,10_000_000,true),12_039_280);
  await account(config,IDS.fees,Buffer.alloc(420),4_000_000);
  const curveBytes=Buffer.alloc(160);Buffer.from([23,183,248,55,96,216,172,96]).copy(curveBytes);manager.publicKey.toBuffer().copy(curveBytes,49);IDS.wsol.toBuffer().copy(curveBytes,83);await account(curve,IDS.pump,curveBytes,3_000_000);
  await account(creatorVault,ZERO,Buffer.alloc(0),890_880);
  const platformBytes=Buffer.alloc(900);Buffer.from([160,78,128,0,248,83,230,160]).copy(platformBytes);IDS.stonk.toBuffer().copy(platformBytes,16);await account(platform,IDS.launchlab,platformBytes,8_000_000);
  const poolBytes=Buffer.alloc(512);Buffer.from([247,237,227,245,215,195,222,70]).copy(poolBytes);platform.toBuffer().copy(poolBytes,173);stonkMint.toBuffer().copy(poolBytes,205);IDS.wsol.toBuffer().copy(poolBytes,237);manager.publicKey.toBuffer().copy(poolBytes,333);await account(launchlabPool,IDS.launchlab,poolBytes,5_000_000);
  const rpc=await rpcPorts();let faucet=await port();while(faucet===rpc||faucet===rpc+1)faucet=await port();
  const validator=spawn('solana-test-validator',['--ledger',join(directory,'ledger'),'--rpc-port',String(rpc),'--faucet-port',String(faucet),'--bind-address','127.0.0.1','--gossip-host','127.0.0.1','--quiet','--upgradeable-program',client.programId.toBase58(),binary('distribution_router'),authority.publicKey.toBase58(),
    '--bpf-program',IDS.token.toBase58(),binary('router_test_spl_token'),
    ...[IDS.pump,IDS.fees,IDS.amm,IDS.launchlab].flatMap(key=>['--bpf-program',key.toBase58(),binary('router_test_launchpad_fixture')]),...fixtures],{stdio:['ignore','pipe','pipe']});
  let log='',spawnError;validator.on('error',e=>{spawnError=e});for(const stream of [validator.stdout,validator.stderr])stream.on('data',b=>{log=(log+b).slice(-6000)});
  t.after(async()=>{if(validator.exitCode===null&&!spawnError){const ended=once(validator,'exit');validator.kill('SIGTERM');await Promise.race([ended,delay(5000)]);if(validator.exitCode===null&&validator.signalCode===null){validator.kill('SIGKILL');await ended;}}await rm(directory,{recursive:true,force:true});});
  const connection=new Connection(`http://127.0.0.1:${rpc}`,{commitment:'confirmed',fetch:(url,options)=>fetch(url,{...options,signal:AbortSignal.timeout(10000)})});let ready=false;
  for(let i=0;i<100;i++){if(spawnError||validator.exitCode!==null)throw Error(`Validator failed: ${spawnError??log}`);try{if(await connection.getSlot()>=5&&(await connection.getAccountInfo(client.programId))?.executable){ready=true;break;}}catch{}await delay(300)}assert.ok(ready,log);
  let nonce=0;
  const send=async(instructions,signers=[authority])=>{
    const tx=new Transaction().add(ComputeBudgetProgram.setComputeUnitLimit({units:650_000+nonce++}),...instructions);tx.recentBlockhash=(await connection.getLatestBlockhash()).blockhash;tx.feePayer=signers[0].publicKey;tx.sign(...signers);
    const signature=await connection.sendRawTransaction(tx.serialize());
    for(let i=0;i<150;i++){const s=(await connection.getSignatureStatuses([signature])).value[0];if(s?.err)throw Error(JSON.stringify(s.err));if(['confirmed','finalized'].includes(s?.confirmationStatus))return signature;await delay(100)}throw Error('Transaction not confirmed');
  };
  const rejected=async(ix,signers,re)=>assert.rejects(()=>send([ix],signers),e=>{assert.match(`${e.message}\n${e.logs?.join('\n')??''}`,re);return true});
  const vstate=async(m=mint)=>decodeVault(await connection.getAccountInfo(client.vaultFor(m)));
  const cstate=async(id,m=mint)=>decodeCampaign(await connection.getAccountInfo(client.campaignFor(m,id)));
  const bstate=async()=>decodeBank(await connection.getAccountInfo(client.bankFor(mint,rewardMint)));
  const tokens=async(address)=>(await connection.getAccountInfo(address)).data.readBigUInt64LE(64);
  const now=async()=>Number((await connection.getAccountInfo(new PublicKey('SysvarC1ock11111111111111111111111111111111'))).data.readBigInt64LE(32));
  const policy={shareBps:[4000,1000,1000,3000,1000],minimumRouteLamports:10000n};
  const register=(m=mint,changes={})=>client.registerVault({authority:authority.publicKey,manager:manager.publicKey,mint:m,activationTreasury:treasury,policy,...changes});
  const allocate=async(m=mint,changes={})=>{const v=await vstate(m);return client.route({mint:m,expectedSequence:v.routeSequence,expectedRulesVersion:v.rulesVersion,...changes})};

  await t.test('bootstrap and canonical vault creation require the correct authorities',async()=>{
    await rejected(client.initializeRouter({authority:outsider.publicKey,activationTreasury:treasury,rewardMint}),[outsider],/InvalidAuthority/);
    await send([client.initializeRouter({authority:authority.publicKey,activationTreasury:treasury,rewardMint})]);
    await rejected(register(mint,{authority:outsider.publicKey}),[outsider,manager],/InvalidAuthority/);
    const before=await connection.getBalance(treasury);await send([register()],[authority,manager]);assert.equal(await connection.getBalance(treasury)-before,200_000_000);
    await rejected(register(),[authority,manager],/already in use/);
  });
  assert.ok(await connection.getAccountInfo(client.router),'Bootstrap must succeed before dependent flow tests');
  await t.test('Pump setup uses creator-signed published ABI and finalizes a vault share',async()=>{
    const p={mint,manager:manager.publicKey,creator:manager.publicKey};
    const wrong=client.createPumpSharing(p);wrong.keys[4].pubkey=IDS.amm;await rejected(wrong,[manager],/InvalidFeeSource/);
    await send([client.createPumpSharing(p)],[manager]);
    await rejected(client.finalizePumpSharing({...p,shareBps:10001}),[manager],/InvalidShares/);
    await send([client.finalizePumpSharing({...p,shareBps:3000})],[manager]);assert.equal((await vstate()).feeShareBps,3000);
    await rejected(client.finalizePumpSharing({...p,shareBps:5000}),[manager],/FeeAuthorityLocked/);
    await send([client.connectPump({mint,manager:manager.publicKey})],[manager]);
  });
  await t.test('creator-fee collection is permissionless, attributable and atomically minimum-output checked',async()=>{
    await send([SystemProgram.transfer({fromPubkey:outsider.publicKey,toPubkey:creatorVault,lamports:1_000_000_000})],[outsider]);
    const p={mint,payer:outsider.publicKey,shareholders:[manager.publicKey,client.vaultFor(mint)]};
    const before=await connection.getBalance(client.vaultFor(mint));
    await rejected(client.collectPumpFees({...p,minimumReceived:300_000_001n}),[outsider],/BelowThreshold/);
    assert.equal(await connection.getBalance(client.vaultFor(mint)),before);
    await send([client.collectPumpFees(p)],[outsider]);assert.equal((await vstate()).verifiedCreatorFees,300_000_000n);
    assert.equal(await connection.getBalance(client.vaultFor(mint))-before,300_000_000);
  });
  let stale;
  await t.test('five allocations keep funds in escrow and exclude already allocated SOL',async()=>{
    await send([client.deposit({contributor:outsider.publicKey,mint,amount:700_000_000n})],[outsider]);
    stale=await allocate();const before=await connection.getBalance(client.vaultFor(mint));await send([stale],[outsider]);
    assert.equal(await connection.getBalance(client.vaultFor(mint)),before);
    assert.deepEqual((await vstate()).balances,[400_000_000n,100_000_000n,100_000_000n,300_000_000n,100_000_000n]);
    await rejected(stale,[outsider],/StaleSequence/);await rejected(await allocate(),[outsider],/InvalidAmount/);
    await rejected(client.withdrawReserve({mint,manager:outsider.publicKey,recipient,amount:1n}),[outsider],/InvalidAuthority/);
    await rejected(client.withdrawReserve({mint,manager:manager.publicKey,recipient,amount:100_000_001n}),[manager],/InsufficientFunds/);
  });
  const solId=evidenceId('sol-campaign'),userId=evidenceId('x:alice'),rulesHash=evidenceId('meaningful original TIPANSEM content v1');
  let solRules;
  const award=(post,amount=30000n,user=userId)=>({postId:evidenceId(post),userId:user,amount,expiresAt:Math.floor(Date.now()/1000)+120,rulesHash});
  const solClaim=(a,changes={})=>client.awardSol({payer:outsider.publicKey,verifier:authority.publicKey,mint,id:solId,recipient,award:a,...changes});
  await t.test('manager creates and funds a bounded native campaign from the campaign bucket',async()=>{
    const time=await now();solRules={verifier:authority.publicKey,rewardMint:ZERO,bucket:0,startsAt:time-5,endsAt:time+3600,minReward:1000n,maxReward:50000n,rollingLimit:2,windowSeconds:3600,dailyLimit:2,maxEpochBudget:1000000n,refillAmount:50000n,refillThreshold:10000n,autoRefill:true,recurring:false,rulesHash};
    await send([client.createCampaign({mint,id:solId,manager:manager.publicKey,rules:solRules})],[manager]);
    await rejected(client.fundCampaignSol({mint,id:solId,actor:outsider.publicKey,amount:60000n}),[outsider],/InvalidAuthority/);
    await send([client.fundCampaignSol({mint,id:solId,actor:manager.publicKey,amount:60000n})],[manager]);assert.equal((await cstate(solId)).remaining,60000n);
    await rejected(client.fundCampaignSol({mint,id:solId,actor:manager.publicKey,amount:1000000n}),[manager],/BudgetExceeded/);
  });
  await t.test('only verified, correctly bounded evidence can release an award',async()=>{
    await rejected(solClaim(award('unauthorized'),{verifier:outsider.publicKey}),[outsider],/InvalidAuthority/);
    await rejected(solClaim({...award('hash'),rulesHash:evidenceId('wrong rules')}),[outsider,authority],/InvalidEvidence/);
    await rejected(solClaim(award('amount',50001n)),[outsider,authority],/InvalidAmount/);
    const before=await connection.getBalance(recipient);await send([solClaim(award('tweet-1'))],[outsider,authority]);assert.equal(await connection.getBalance(recipient)-before,30000);
  });
  await t.test('a post cannot pay twice, even when resubmitted under another identity',async()=>{
    await rejected(solClaim(award('tweet-1')),[outsider,authority],/already in use/);
    await rejected(solClaim(award('tweet-1',1000n,evidenceId('another user'))),[outsider,authority],/already in use/);
    await send([solClaim(award('tweet-2',25000n))],[outsider,authority]);
    await rejected(solClaim(award('tweet-3',1000n)),[outsider,authority],/InvalidLimit/);
    assert.equal((await cstate(solId)).remaining,5000n);
  });
  await t.test('exhausted budgets roll back receipts and automatic refill has a fixed amount',async()=>{
    const failed=award('unfunded',10000n,evidenceId('x:bob'));
    await rejected(solClaim(failed),[outsider,authority],/InsufficientFunds/);
    assert.equal(await connection.getAccountInfo(derive(client.programId,'reward',client.campaignFor(mint,solId),failed.postId)),null);
    await send([client.refillCampaignSol({mint,id:solId,actor:outsider.publicKey})],[outsider]);assert.equal((await cstate(solId)).remaining,55000n);
    await rejected(client.refillCampaignSol({mint,id:solId,actor:outsider.publicKey}),[outsider],/BelowThreshold/);
  });
  await t.test('canceling a native campaign refunds only that escrow to its source bucket',async()=>{
    await rejected(client.closeCampaignSol({mint,id:solId,actor:outsider.publicKey}),[outsider],/InvalidAuthority/);
    const before=(await vstate()).balances[0];await send([client.closeCampaignSol({mint,id:solId,actor:manager.publicKey})],[manager]);assert.equal((await vstate()).balances[0]-before,55000n);
    assert.equal((await cstate(solId)).status,2);await rejected(solClaim(award('closed',1000n,evidenceId('new'))),[outsider,authority],/Paused/);
  });
  for(const [name,limits] of [
    ['rolling',{rollingLimit:1,dailyLimit:60,lifetimeLimit:0}],
    ['daily',{rollingLimit:5,dailyLimit:1,lifetimeLimit:0}],
    ['lifetime',{rollingLimit:5,dailyLimit:60,lifetimeLimit:1}],
  ])await t.test(`${name} limit independently rejects another award without consuming its receipt`,async()=>{
    const id=evidenceId(`limit-${name}`),time=await now(),rules={...solRules,...limits,startsAt:time-5,endsAt:time+3600};
    await send([client.createCampaign({mint,id,manager:manager.publicKey,rules}),client.fundCampaignSol({mint,id,actor:manager.publicKey,amount:10000n})],[manager]);
    await send([solClaim(award(`${name}-1`,1000n),{id})],[outsider,authority]);
    const second=award(`${name}-2`,1000n);
    await rejected(solClaim(second,{id}),[outsider,authority],/InvalidLimit/);
    assert.equal(await connection.getAccountInfo(derive(client.programId,'reward',client.campaignFor(mint,id),second.postId)),null);
    assert.equal((await cstate(id)).remaining,9000n);
    await send([client.closeCampaignSol({mint,id,actor:manager.publicKey})],[manager]);
  });
  const orderId=evidenceId('purchase-1');
  await t.test('reward-token bank uses real SPL Token CPI; purchases lock only their allocation',async()=>{
    await send([client.initializeTokenBank({mint,rewardMint,payer:outsider.publicKey})],[outsider]);
    const sourceBefore=await tokens(executorTokens);
    await rejected(client.depositTokens({mint,rewardMint,contributor:outsider.publicKey,source:executorTokens,amount:10n,bucket:3}),[outsider],/InvalidBucket/);
    assert.equal(await tokens(executorTokens),sourceBefore,'Unsupported SPL buckets cannot lock contributor funds');
    await send([client.createPurchase({mint,id:orderId,manager:manager.publicKey,amount:100000000n,minimumTokens:1000n,expiresAt:(await now())+3600})],[manager]);
    assert.equal((await vstate()).balances[3],200000000n);
    const before=await connection.getBalance(client.orderFor(mint,orderId));
    await rejected(client.fillPurchase({mint,id:orderId,rewardMint,executor:outsider.publicKey,source:executorTokens,tokenAmount:999n}),[outsider],/SlippageExceeded/);
    assert.equal(await connection.getBalance(client.orderFor(mint,orderId)),before);assert.equal(await tokens(client.bankTokensFor(mint,rewardMint)),0n);
  });
  await t.test('SOL is released only after the minimum TIPANSEM tokens arrive; orders settle once',async()=>{
    const fill=client.fillPurchase({mint,id:orderId,rewardMint,executor:outsider.publicKey,source:executorTokens,tokenAmount:1000n});await send([fill],[outsider]);
    assert.equal((await bstate()).balances[0],1000n);assert.equal(await tokens(client.bankTokensFor(mint,rewardMint)),1000n);
    await rejected(fill,[outsider],/Closed/);
  });
  await t.test('unfilled purchases return SOL to their project, not to the caller',async()=>{
    const id=evidenceId('cancel-order'),before=(await vstate()).balances[3];
    await send([client.createPurchase({mint,id,manager:manager.publicKey,amount:10000000n,minimumTokens:10n,expiresAt:(await now())+3600})],[manager]);
    await rejected(client.cancelPurchase({mint,id,actor:outsider.publicKey}),[outsider],/InvalidAuthority/);
    await send([client.cancelPurchase({mint,id,actor:manager.publicKey})],[manager]);assert.equal((await vstate()).balances[3],before);
  });
  const tokenId=evidenceId('token-campaign');
  const tokenClaim=(post,amount=100n,changes={})=>client.awardTokens({mint,id:tokenId,rewardMint,payer:outsider.publicKey,verifier:authority.publicKey,recipient,recipientTokens,award:award(post,amount,evidenceId('x:token-user')),...changes});
  await t.test('campaign escrows purchased tokens with a capped automatic-refill policy',async()=>{
    const time=await now();const rules={...solRules,rewardMint,startsAt:time-5,endsAt:time+3600,minReward:10n,maxReward:100n,rollingLimit:5,dailyLimit:60,maxEpochBudget:1000n,refillAmount:200n,refillThreshold:20n};
    await send([client.createCampaign({mint,id:tokenId,manager:manager.publicKey,rules}),client.initializeCampaignTokens({mint,id:tokenId,rewardMint,payer:manager.publicKey})],[manager]);
    await send([client.fundCampaignTokens({mint,id:tokenId,rewardMint,actor:manager.publicKey,amount:300n})],[manager]);assert.equal(await tokens(client.campaignTokensFor(mint,tokenId)),300n);assert.equal((await bstate()).balances[0],700n);
  });
  await t.test('SPL reward payout rejects substituted wallets and pays verified claims exactly',async()=>{
    await rejected(tokenClaim('bad-token-recipient',100n,{recipientTokens:executorTokens}),[outsider,authority],/ConstraintTokenOwner/);
    for(const post of ['token-1','token-2','token-3'])await send([tokenClaim(post)],[outsider,authority]);
    assert.equal(await tokens(recipientTokens),300n);assert.equal((await cstate(tokenId)).remaining,0n);
    await rejected(tokenClaim('token-1'),[outsider,authority],/already in use/);
  });
  await t.test('token campaign refill and refund conserve the bank and reward escrow',async()=>{
    await send([client.refillCampaignTokens({mint,id:tokenId,rewardMint,actor:outsider.publicKey})],[outsider]);assert.equal((await cstate(tokenId)).remaining,200n);
    await send([client.closeCampaignTokens({mint,id:tokenId,rewardMint,actor:manager.publicKey})],[manager]);assert.equal(await tokens(client.campaignTokensFor(mint,tokenId)),0n);assert.equal((await bstate()).balances[0],700n);
    assert.equal(await tokens(client.bankTokensFor(mint,rewardMint)),700n);
  });
  await t.test('Stonk verifies the actual platform and creator; unrelated wallets cannot relay as creator',async()=>{
    await send([register(stonkMint)],[authority,manager]);
    const p={mint:stonkMint,manager:manager.publicKey,creator:manager.publicKey,pool:launchlabPool,platform,shareBps:3000};
    await rejected(client.connectLaunchlab({...p,creator:outsider.publicKey}),[manager,outsider],/InvalidAuthority/);
    await send([client.connectLaunchlab(p)],[manager]);assert.equal((await vstate(stonkMint)).feeKind,2);
    await rejected(client.relayLaunchlabFees({mint:stonkMint,creator:outsider.publicKey,amount:10000n}),[outsider],/InvalidAuthority/);
  });
  await t.test('LaunchLab claim allocates only newly claimed WSOL and returns buffer rent',async()=>{
    const before=await connection.getBalance(client.vaultFor(stonkMint));
    await send([client.claimLaunchlabFees({mint:stonkMint,creator:manager.publicKey,payer:outsider.publicKey,minimumReceived:3000000n})],[outsider,manager]);
    assert.equal(await connection.getBalance(client.vaultFor(stonkMint))-before,3000000);
    assert.equal(await tokens(creatorTokens),12000000n);assert.equal(await tokens(launchlabFeeVault),0n);
    assert.equal((await vstate(stonkMint)).verifiedCreatorFees,0n,'creator-wide claims must not masquerade as per-mint Pump fees');
    await send([client.relayLaunchlabFees({mint:stonkMint,creator:manager.publicKey,amount:10000n})],[manager]);
  });
  await t.test('recurring epochs advance permissionlessly but cannot reset post receipts or unspent budgets',async()=>{
    t.diagnostic('Creating short recurring campaign');
    const id=evidenceId('recurring'),time=await now(),rules={...solRules,startsAt:time-1,endsAt:time+8,recurring:true};
    await send([client.createCampaign({mint,id,manager:manager.publicKey,rules}),client.fundCampaignSol({mint,id,actor:manager.publicKey,amount:10000n})],[manager]);
    const paid=solClaim(award('recurring-post',1000n),{id});
    await send([paid],[outsider,authority]);
    t.diagnostic('Checking early epoch rollover');
    await rejected(client.advanceCampaignEpoch({mint,id}),[outsider],/OutsideWindow/);
    for(let i=0;i<100&&(await now())<rules.endsAt;i++)await delay(100);
    t.diagnostic('Advancing expired epoch');
    await send([client.advanceCampaignEpoch({mint,id})],[outsider]);assert.equal((await cstate(id)).epoch,1n);
    assert.equal((await cstate(id)).remaining,9000n);assert.equal((await cstate(id)).epochFunded,9000n);
    await rejected(paid,[outsider,authority],/already in use/);
    await rejected(client.advanceCampaignEpoch({mint,id}),[outsider],/OutsideWindow/);
    await rejected(client.closeCampaignSol({mint,id,actor:outsider.publicKey}),[outsider],/InvalidAuthority/);
  });
  await t.test('Rust keeper fills only its approved inventory order then refills the configured campaign',async()=>{
    const id=evidenceId('keeper-campaign'),purchase=evidenceId('keeper-order'),time=await now();
    const rules={...solRules,rewardMint,startsAt:time-5,endsAt:time+3600,minReward:1n,maxReward:10n,maxEpochBudget:100n,refillAmount:10n,refillThreshold:1n};
    await send([client.createPurchase({mint,id:purchase,manager:manager.publicKey,amount:10000n,minimumTokens:10n,expiresAt:time+3600}),client.createCampaign({mint,id,manager:manager.publicKey,rules})],[manager]);
    await send([client.initializeCampaignTokens({mint,id,rewardMint,payer:manager.publicKey})],[manager]);
    // The keeper deliberately reads finalized state, unlike the fast confirmed
    // assertions above. Do not make it interpret a not-yet-rooted account as missing.
    let rooted=false;
    for(let i=0;i<300;i++){
      if(await connection.getAccountInfo(client.campaignTokensFor(mint,id),'finalized')){rooted=true;break;}
      await delay(200);
    }
    assert.ok(rooted,'Keeper fixture accounts must reach finalized commitment');
    const configPath=join(directory,'keeper-config.json'),walletPath=join(directory,'keeper-keypair.json');
    await writeFile(walletPath,JSON.stringify([...outsider.secretKey]));
    await writeFile(configPath,JSON.stringify({rpc_url:`http://127.0.0.1:${rpc}`,program_id:client.programId.toBase58(),projects:[{mint:mint.toBase58(),campaigns:[id.toString('hex')],purchase_source_token_account:executorTokens.toBase58(),purchases:[{id:purchase.toString('hex'),maximum_tokens:10,minimum_sol_lamports:10000}]}]}));
    const run=async(command)=>{
      const child=spawn(keeperBinary,[command,configPath,...(command==='inspect'?[]:[walletPath])],{stdio:['ignore','pipe','pipe']});
      let output='';for(const stream of [child.stdout,child.stderr])stream.on('data',b=>{output+=b});
      const [code]=await once(child,'exit');assert.equal(code,0,output);return output;
    };
    const sourceBefore=await tokens(executorTokens);
    await run('inspect');assert.equal((await cstate(id)).remaining,0n);assert.equal(await tokens(executorTokens),sourceBefore);
    await run('tick');assert.equal((await cstate(id)).remaining,10n);assert.equal(await tokens(executorTokens),sourceBefore-10n);
    await run('tick');assert.equal((await cstate(id)).remaining,10n);assert.equal(await tokens(executorTokens),sourceBefore-10n);
  });
  await t.test('pause, stale allocation versions and two-step manager rotation remain enforced',async()=>{
    await send([client.deposit({contributor:outsider.publicKey,mint,amount:100000n})],[outsider]);const old=await allocate();
    await send([client.configureVault({mint,manager:manager.publicKey,policy})],[manager]);await rejected(old,[outsider],/StalePolicy/);
    await send([client.pauseVault({mint,manager:manager.publicKey,paused:true})],[manager]);await rejected(await allocate(),[outsider],/Paused/);
    await send([client.pauseVault({mint,manager:manager.publicKey,paused:false})],[manager]);await send([await allocate()],[outsider]);
    await rejected(client.acceptManager({mint,nextManager:outsider.publicKey}),[outsider],/InvalidAuthority/);
    await send([client.proposeManager({mint,manager:manager.publicKey,nextManager:replacement.publicKey})],[manager]);await send([client.acceptManager({mint,nextManager:replacement.publicKey})],[replacement]);
    await rejected(client.pauseVault({mint,manager:manager.publicKey,paused:true}),[manager],/InvalidAuthority/);
  });
});
