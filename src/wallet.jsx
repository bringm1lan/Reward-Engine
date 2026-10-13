import { createContext, useCallback, useContext, useMemo, useState } from 'react';
import { ConnectionProvider, WalletProvider, useConnection, useWallet } from '@solana/wallet-adapter-react';
import { Wallet, X } from 'lucide-react';
import { useToast } from './toast';

const PickerContext = createContext(null);
const NO_BUNDLED_ADAPTERS = [];

export function SolanaProvider({ children }) {
  const { notify } = useToast();
  const [open, setOpen] = useState(false);
  const endpoint = import.meta.env.VITE_SOLANA_RPC_URL?.trim() || 'https://api.devnet.solana.com';
  const onError = useCallback((error) => {
    if (['WalletNotSelectedError', 'WalletNotReadyError', 'WalletSignMessageError'].includes(error?.name)) return;
    notify({ variant: 'error', title: 'Wallet could not connect', description: error?.message || 'Try another wallet.' });
  }, [notify]);
  const picker = useMemo(() => ({ open: () => setOpen(true), close: () => setOpen(false) }), []);

  return (
    <ConnectionProvider endpoint={endpoint} config={{ commitment: 'confirmed' }}>
      <WalletProvider wallets={NO_BUNDLED_ADAPTERS} autoConnect localStorageKey="ansem:distribution-wallet" onError={onError}>
        <PickerContext.Provider value={picker}>
          {children}
          {open && <WalletPicker onClose={picker.close} />}
        </PickerContext.Provider>
      </WalletProvider>
    </ConnectionProvider>
  );
}

function WalletPicker({ onClose }) {
  const { wallets, select } = useWallet();
  const available = wallets.filter(({ readyState }) => readyState === 'Installed' || readyState === 'Loadable');
  const choose = (name) => {
    select(name);
    onClose();
  };

  return (
    <div className="wallet-scrim" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="wallet-picker" role="dialog" aria-modal="true" aria-labelledby="wallet-picker-title">
        <header><div><span>MANAGER WALLET</span><h2 id="wallet-picker-title">Choose a Solana wallet</h2></div><button type="button" onClick={onClose} aria-label="Close"><X /></button></header>
        {available.length ? (
          <div className="wallet-options">
            {available.map(({ adapter }) => (
              <button type="button" key={adapter.name} onClick={() => choose(adapter.name)}>
                <img src={adapter.icon} alt="" /><strong>{adapter.name}</strong><span>Connect</span>
              </button>
            ))}
          </div>
        ) : (
          <div className="wallet-empty"><Wallet /><strong>No wallet detected</strong><p>Open this page in a browser with Phantom, Solflare, Backpack, or another Wallet Standard wallet installed.</p></div>
        )}
      </section>
    </div>
  );
}

export function useSolanaWallet() {
  const { connection } = useConnection();
  const {
    wallet,
    publicKey,
    connected,
    connecting,
    disconnecting,
    disconnect,
    sendTransaction,
    signMessage,
  } = useWallet();
  const picker = useContext(PickerContext);
  if (!picker) throw new Error('useSolanaWallet must be used within SolanaProvider');

  return {
    connection,
    publicKey,
    address: publicKey?.toBase58() ?? null,
    isConnected: connected && Boolean(publicKey),
    isConnecting: connecting,
    isDisconnecting: disconnecting,
    walletName: wallet?.adapter.name ?? null,
    walletIcon: wallet?.adapter.icon ?? null,
    connect: picker.open,
    disconnect: () => disconnect().catch(() => {}),
    sendTransaction,
    signMessage,
  };
}
