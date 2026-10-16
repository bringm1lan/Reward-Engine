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
import { createRouterClient, decodeVault } from '../client.mjs';

const client = createRouterClient();
const binary = fileURLToPath(new URL('../../../target/deploy/distribution_router.so', import.meta.url));
const TOKEN_PROGRAM = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

async function availablePort() {
  const server = createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return port;
}

async function rpcPortPair() {
  for (let attempt = 0; attempt < 10; attempt++) {
    const port = await availablePort();
    const server = createServer();
    try {
      await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port + 1, '127.0.0.1', resolve); });
      await new Promise((resolve) => server.close(resolve));
      return port;
    } catch { /* Try another pair if the websocket port is occupied. */ }
  }
  throw new Error('No available loopback RPC/websocket port pair');
}

test('distribution router: real local SBF transactions', { timeout: 180_000 }, async (t) => {
  await readFile(binary);
  const directory = await mkdtemp(join(tmpdir(), 'ansem-v3-router-'));
  const authority = Keypair.generate();
  const manager = Keypair.generate();
  const outsider = Keypair.generate();
  const replacement = Keypair.generate();
  const activationTreasury = Keypair.generate().publicKey;
  const recipients = Array.from({ length: 4 }, () => Keypair.generate().publicKey);
  const policy = { recipients, shareBps: [6000, 2000, 1000, 1000], minimumRouteLamports: 10_000n };
  const genesisAccounts = [authority.publicKey, manager.publicKey, outsider.publicKey, replacement.publicKey, activationTreasury, ...recipients];
  const fixtureArgs = [];
  for (let index = 0; index < genesisAccounts.length; index++) {
    const accountPath = join(directory, `account-${index}.json`);
    await writeFile(accountPath, JSON.stringify({
      pubkey: genesisAccounts[index].toBase58(),
      account: { lamports: 20_000_000_000, data: ['', 'base64'], owner: SystemProgram.programId.toBase58(), executable: false, rentEpoch: 0, space: 0 },
    }));
    fixtureArgs.push('--account', genesisAccounts[index].toBase58(), accountPath);
  }
  // A valid initialized SPL mint fixture. Routing does not invoke the token
  // program, so the test does not need to clone any program from a public RPC.
  const mint = Keypair.generate();
  const mintBytes = Buffer.alloc(82);
  mintBytes.writeUInt32LE(1, 0);
  authority.publicKey.toBuffer().copy(mintBytes, 4);
  mintBytes[44] = 9;
  mintBytes[45] = 1;
  const mintPath = join(directory, 'mint.json');
  await writeFile(mintPath, JSON.stringify({
    pubkey: mint.publicKey.toBase58(),
    account: { lamports: 2_000_000, data: [mintBytes.toString('base64'), 'base64'], owner: TOKEN_PROGRAM.toBase58(), executable: false, rentEpoch: 0, space: 82 },
  }));
  fixtureArgs.push('--account', mint.publicKey.toBase58(), mintPath);
  const rpcPort = await rpcPortPair();
  let faucetPort = await availablePort();
  while (faucetPort === rpcPort || faucetPort === rpcPort + 1) faucetPort = await availablePort();
  const validator = spawn('solana-test-validator', [
    '--ledger', join(directory, 'ledger'), '--rpc-port', String(rpcPort), '--faucet-port', String(faucetPort),
    '--bind-address', '127.0.0.1', '--gossip-host', '127.0.0.1', '--quiet',
    '--upgradeable-program', client.programId.toBase58(), binary, authority.publicKey.toBase58(),
    ...fixtureArgs,
  ], { stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '';
  let spawnError;
  validator.on('error', (error) => { spawnError = error; });
  validator.stdout.on('data', (chunk) => { output = (output + chunk).slice(-6000); });
  validator.stderr.on('data', (chunk) => { output = (output + chunk).slice(-6000); });
  t.after(async () => {
    if (validator.exitCode === null && !spawnError) {
      const exited = once(validator, 'exit');
      validator.kill('SIGTERM');
      await Promise.race([exited, delay(5000)]);
      if (validator.exitCode === null && validator.signalCode === null) {
        validator.kill('SIGKILL');
        await exited;
      }
    }
    await rm(directory, { recursive: true, force: true });
  });
  const connection = new Connection(`http://127.0.0.1:${rpcPort}`, 'confirmed');
  let ready = false;
  for (let attempt = 0; attempt < 120; attempt++) {
    if (spawnError || validator.exitCode !== null) throw new Error(`Local validator failed: ${spawnError ?? output}`);
    try { await connection.getLatestBlockhash(); ready = true; break; } catch { await delay(500); }
  }
  assert.ok(ready, `Local validator did not start: ${output}`);
  // Vary the message so rejected replays exercise the program instead of hitting
  // the validator's successful-transaction signature cache within one blockhash.
  let transactionNumber = 0;
  const send = async (instructions, signers = [authority]) => {
    const transaction = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 250_000 + transactionNumber++ }), ...instructions,
    );
    transaction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    transaction.feePayer = signers[0].publicKey;
    transaction.sign(...signers);
    const signature = await connection.sendRawTransaction(transaction.serialize());
    // HTTP confirmation keeps this isolated harness independent of websocket
    // reconnect timers after the temporary validator shuts down.
    for (let attempt = 0; attempt < 150; attempt++) {
      const status = (await connection.getSignatureStatuses([signature])).value[0];
      if (status?.err) throw new Error(`Local transaction failed: ${JSON.stringify(status.err)}`);
      if (status?.confirmationStatus === 'confirmed' || status?.confirmationStatus === 'finalized') return signature;
      await delay(100);
    }
    throw new Error(`Local transaction was not confirmed: ${signature}`);
  };
  const rejected = async (instruction, signers, expectedLog) => {
    await assert.rejects(() => send([instruction], signers), (error) => {
      assert.match(`${error.message}\n${error.logs?.join('\n') ?? ''}`, expectedLog);
      return true;
    });
  };
  const vaultAddress = client.vaultFor(mint.publicKey);
  const register = (changes = {}) => client.registerVault({ authority: authority.publicKey, manager: manager.publicKey, mint: mint.publicKey, activationTreasury, policy, ...changes });
  const state = async () => decodeVault(await connection.getAccountInfo(vaultAddress));
  const route = async (changes = {}) => {
    const current = await state();
    return client.route({ mint: mint.publicKey, recipients: current.policy.recipients, expectedSequence: current.routeSequence, expectedRulesVersion: current.rulesVersion, ...changes });
  };

  await t.test('outsiders cannot bootstrap router config', async () => {
    await rejected(client.initializeRouter({ authority: outsider.publicKey, activationTreasury }), [outsider], /InvalidAuthority/);
    await send([client.initializeRouter({ authority: authority.publicKey, activationTreasury })]);
  });

  await t.test('canonical registration needs verified registrar and manager', async () => {
    await rejected(register({ authority: outsider.publicKey }), [outsider, manager], /InvalidAuthority/);
    await rejected(register({ mint: outsider.publicKey }), [authority, manager], /AccountOwnedByWrongProgram/);
    const before = await connection.getBalance(activationTreasury);
    await send([register()], [authority, manager]);
    assert.equal(await connection.getBalance(activationTreasury) - before, 200_000_000);
    const current = await state();
    assert.ok(current.manager.equals(manager.publicKey));
    assert.deepEqual(current.policy.shareBps, policy.shareBps);
    await rejected(register(), [authority, manager], /already in use/);
  });

  await t.test('only current manager can configure recipients', async () => {
    await rejected(client.configureVault({ manager: outsider.publicKey, mint: mint.publicKey, policy }), [outsider], /InvalidAuthority/);
    await rejected(client.configureVault({ manager: manager.publicKey, mint: mint.publicKey, policy: { ...policy, recipients: [recipients[0], recipients[0], recipients[2], recipients[3]] } }), [manager], /DuplicateRecipient/);
    await rejected(client.configureVault({ manager: manager.publicKey, mint: mint.publicKey, policy: { ...policy, recipients: [vaultAddress, ...recipients.slice(1)] } }), [manager], /InvalidRecipient/);
  });

  await t.test('contributions and raw creator-style SOL transfers both accumulate', async () => {
    await send([client.deposit({ contributor: outsider.publicKey, mint: mint.publicKey, amount: 600_000n })], [outsider]);
    await send([SystemProgram.transfer({ fromPubkey: outsider.publicKey, toPubkey: vaultAddress, lamports: 400_003 })], [outsider]);
    assert.equal((await state()).totalRoutedLamports, 0n);
  });

  let staleInstruction;
  await t.test('permissionless routing conserves funds, retains rent and forbids substituted recipients', async () => {
    const balances = await Promise.all(recipients.map((recipient) => connection.getBalance(recipient)));
    await rejected(await route({ recipients: [outsider.publicKey, ...recipients.slice(1)] }), [outsider], /InvalidRecipient/);
    staleInstruction = await route();
    await send([staleInstruction], [outsider]);
    const balancesAfter = await Promise.all(recipients.map((recipient) => connection.getBalance(recipient)));
    assert.deepEqual(balancesAfter.map((value, index) => value - balances[index]), [600001, 200000, 100000, 100002]);
    const info = await connection.getAccountInfo(vaultAddress);
    assert.equal(info.lamports, await connection.getMinimumBalanceForRentExemption(info.data.length));
    assert.equal((await state()).totalRoutedLamports, 1_000_003n);
  });

  await t.test('retries cannot route a later deposit with an old batch sequence', async () => {
    await send([client.deposit({ contributor: outsider.publicKey, mint: mint.publicKey, amount: 500_000n })], [outsider]);
    await rejected(staleInstruction, [outsider], /StaleSequence/);
    assert.equal((await state()).routeSequence, 1n);
  });

  await t.test('policy revisions invalidate prepared route requests', async () => {
    const old = await route();
    await send([client.configureVault({ manager: manager.publicKey, mint: mint.publicKey, policy })], [manager]);
    await rejected(old, [outsider], /StalePolicy/);
  });

  await t.test('manager and registrar pauses block routing while deposits remain possible', async () => {
    await send([client.pauseVault({ manager: manager.publicKey, mint: mint.publicKey, paused: true })], [manager]);
    await rejected(await route(), [outsider], /Paused/);
    await send([client.deposit({ contributor: outsider.publicKey, mint: mint.publicKey, amount: 1n })], [outsider]);
    await send([client.pauseVault({ manager: manager.publicKey, mint: mint.publicKey, paused: false })], [manager]);
    await send([client.configureRouter({ authority: authority.publicKey, activationTreasury, activationFeeLamports: 200_000_000n, paused: true })]);
    await rejected(await route(), [outsider], /Paused/);
    await send([client.configureRouter({ authority: authority.publicKey, activationTreasury, activationFeeLamports: 200_000_000n, paused: false })]);
  });

  await t.test('two-step management handover prevents unsolicited takeover', async () => {
    await rejected(client.acceptManager({ nextManager: outsider.publicKey, mint: mint.publicKey }), [outsider], /InvalidAuthority/);
    await send([client.proposeManager({ manager: manager.publicKey, mint: mint.publicKey, nextManager: replacement.publicKey })], [manager]);
    await rejected(client.acceptManager({ nextManager: outsider.publicKey, mint: mint.publicKey }), [outsider], /InvalidAuthority/);
    await send([client.acceptManager({ nextManager: replacement.publicKey, mint: mint.publicKey })], [replacement]);
    await rejected(client.pauseVault({ manager: manager.publicKey, mint: mint.publicKey, paused: true }), [manager], /InvalidAuthority/);
    assert.ok((await state()).manager.equals(replacement.publicKey));
  });

  await t.test('minimum batch amount blocks dust routing', async () => {
    await send([await route()], [outsider]);
    await send([client.deposit({ contributor: outsider.publicKey, mint: mint.publicKey, amount: 9999n })], [outsider]);
    await rejected(await route(), [outsider], /BelowThreshold/);
  });
});
