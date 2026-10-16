// Node worker/local-test client. Builds unsigned instructions only.
// The website does not import this file or acquire signing keys.
import { createHash } from 'node:crypto';
import { PublicKey, SystemProgram, TransactionInstruction } from '@solana/web3.js';

export const DEVELOPMENT_PROGRAM_ID = new PublicKey('Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkgMQHGJkY4KT');
const LOADER = new PublicKey('BPFLoaderUpgradeab1e11111111111111111111111');
const pk = (value) => new PublicKey(value);
const meta = (pubkey, isSigner = false, isWritable = false) => ({ pubkey: pk(pubkey), isSigner, isWritable });
const discriminator = (namespace, name) => createHash('sha256').update(`${namespace}:${name}`).digest().subarray(0, 8);

function u64(value) {
  if (typeof value === 'number' && !Number.isSafeInteger(value)) throw new Error('Use bigint for lamports and counters');
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(BigInt(value));
  return bytes;
}

function encodePolicy({ recipients, shareBps, minimumRouteLamports }) {
  if (recipients.length !== 4 || shareBps.length !== 4) throw new Error('Exactly four allocation buckets are required');
  if (shareBps.some((part) => !Number.isInteger(part) || part < 0 || part > 10000) || shareBps.reduce((sum, part) => sum + part, 0) !== 10000) {
    throw new Error('Allocation shares must total 10000 basis points');
  }
  if (BigInt(minimumRouteLamports) <= 0n) throw new Error('Minimum route amount must be positive');
  const shares = Buffer.alloc(8);
  shareBps.forEach((part, index) => shares.writeUInt16LE(part, index * 2));
  return Buffer.concat([...recipients.map((recipient) => pk(recipient).toBuffer()), shares, u64(minimumRouteLamports)]);
}

export function createRouterClient(programId = DEVELOPMENT_PROGRAM_ID) {
  programId = pk(programId);
  const router = PublicKey.findProgramAddressSync([Buffer.from('distribution_router')], programId)[0];
  const vaultFor = (mint) => PublicKey.findProgramAddressSync([Buffer.from('distribution_vault'), pk(mint).toBuffer()], programId)[0];
  const instruction = (name, keys, ...args) => new TransactionInstruction({ programId, keys, data: Buffer.concat([discriminator('global', name), ...args]) });
  const manage = (name, manager, mint, ...args) => instruction(name, [meta(manager, true), meta(vaultFor(mint), false, true)], ...args);
  const admin = (name, authority, ...args) => instruction(name, [meta(authority, true), meta(router, false, true)], ...args);

  return {
    programId,
    router,
    vaultFor,
    initializeRouter: ({ authority, activationTreasury }) => instruction('initialize_router', [
      meta(authority, true, true), meta(programId),
      meta(PublicKey.findProgramAddressSync([programId.toBuffer()], LOADER)[0]),
      meta(activationTreasury), meta(router, false, true), meta(SystemProgram.programId),
    ]),
    configureRouter: ({ authority, activationTreasury, activationFeeLamports, paused }) => admin('configure_router', authority,
      pk(activationTreasury).toBuffer(), u64(activationFeeLamports), Buffer.from([paused ? 1 : 0])),
    registerVault: ({ authority, manager, mint, activationTreasury, policy }) => instruction('register_vault', [
      meta(authority, true), meta(router), meta(manager, true, true), meta(mint),
      meta(activationTreasury, false, true), meta(vaultFor(mint), false, true), meta(SystemProgram.programId),
    ], encodePolicy(policy)),
    deposit: ({ contributor, mint, amount }) => instruction('deposit', [
      meta(contributor, true, true), meta(vaultFor(mint), false, true), meta(SystemProgram.programId),
    ], u64(amount)),
    route: ({ mint, recipients, expectedSequence, expectedRulesVersion, minimumAmount = 1n }) => {
      if (recipients.length !== 4) throw new Error('Exactly four recipient accounts are required');
      return instruction('route', [meta(router), meta(vaultFor(mint), false, true), ...recipients.map((address) => meta(address, false, true))],
        u64(expectedSequence), u64(expectedRulesVersion), u64(minimumAmount));
    },
    configureVault: ({ manager, mint, policy }) => manage('configure_vault', manager, mint, encodePolicy(policy)),
    pauseVault: ({ manager, mint, paused }) => manage('pause_vault', manager, mint, Buffer.from([paused ? 1 : 0])),
    proposeManager: ({ manager, mint, nextManager }) => manage('propose_manager', manager, mint, pk(nextManager).toBuffer()),
    acceptManager: ({ nextManager, mint }) => manage('accept_manager', nextManager, mint),
    proposeAuthority: ({ authority, nextAuthority }) => admin('propose_authority', authority, pk(nextAuthority).toBuffer()),
    acceptAuthority: ({ nextAuthority }) => admin('accept_authority', nextAuthority),
  };
}

export function decodeVault(account, programId = DEVELOPMENT_PROGRAM_ID) {
  const bytes = account?.data;
  if (!account?.owner.equals(pk(programId)) || !bytes || bytes.length !== 274 || !bytes.subarray(0, 8).equals(discriminator('account', 'DistributionVault'))) {
    throw new Error('Not a DistributionVault account owned by this router');
  }
  let offset = 8;
  const key = () => { const result = new PublicKey(bytes.subarray(offset, offset + 32)); offset += 32; return result; };
  const integer = () => { const result = bytes.readBigUInt64LE(offset); offset += 8; return result; };
  const mint = key();
  const manager = key();
  const pendingManager = key();
  const recipients = Array.from({ length: 4 }, key);
  const shareBps = Array.from({ length: 4 }, () => { const part = bytes.readUInt16LE(offset); offset += 2; return part; });
  const minimumRouteLamports = integer();
  return {
    mint, manager, pendingManager,
    policy: { recipients, shareBps, minimumRouteLamports },
    rulesVersion: integer(), routeSequence: integer(), totalRoutedLamports: integer(),
    paused: bytes[offset++] !== 0, bump: bytes[offset],
  };
}
