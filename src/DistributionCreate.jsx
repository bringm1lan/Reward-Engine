import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  BadgeCheck,
  CircleDollarSign,
  Coins,
  ExternalLink,
  FileSignature,
  KeyRound,
  Link2,
  Rocket,
  ShieldCheck,
  Wallet,
} from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import { useAuth } from './auth';
import { useToast } from './toast';
import { useAsyncResource } from './useAsyncResource';
import { useSolanaWallet } from './wallet';
import { api } from './api';
import { makeIdempotencyKey, ProjectMark, shortAddress } from './components';
import './distribution.css';

const initialForm = {
  mint: '',
  name: '',
  symbol: '',
  description: '',
  logoUrl: '',
  websiteUrl: '',
  xHandle: '',
  launchpad: 'pump_fun',
  launchpadUrl: '',
  managerMode: 'creator',
};

const signatureToBase64 = (signature) => {
  let binary = '';
  for (const byte of signature) binary += String.fromCharCode(byte);
  return globalThis.btoa(binary);
};

export function DistributionCreate() {
  const { isLoggedIn, signIn } = useAuth();
  const wallet = useSolanaWallet();
  const navigate = useNavigate();
  const { notify } = useToast();
  const [form, setForm] = useState(initialForm);
  const [project, setProject] = useState(null);
  const [creating, setCreating] = useState(false);
  const [activating, setActivating] = useState(false);
  const [error, setError] = useState('');

  const fetchManaged = useCallback(
    (signal) => api.getManagedDistributionProjects({ signal }),
    [],
  );
  const managed = useAsyncResource(fetchManaged, { enabled: isLoggedIn });

  useEffect(() => {
    const previous = document.title;
    document.title = 'Connect a token · Ansem.tips';
    return () => { document.title = previous; };
  }, []);

  useEffect(() => {
    if (project) return;
    const pending = managed.data?.projects?.find((item) => item.status === 'pending_activation');
    if (pending) setProject(pending);
  }, [managed.data, project]);

  const setField = (key) => (event) => {
    const value = event.target.value;
    setForm((current) => ({ ...current, [key]: key === 'symbol' ? value.toUpperCase() : value }));
  };

  const canCreate = useMemo(
    () => Boolean(
      isLoggedIn &&
      wallet.isConnected &&
      wallet.signMessage &&
      form.mint.trim() &&
      form.name.trim() &&
      form.symbol.trim() &&
      !creating
    ),
    [creating, form.mint, form.name, form.symbol, isLoggedIn, wallet.isConnected, wallet.signMessage],
  );

  const create = async (event) => {
    event.preventDefault();
    if (!canCreate) return;
    setCreating(true);
    setError('');
    try {
      const challenge = await api.createDistributionChallenge({
        mint: form.mint.trim(),
        wallet: wallet.address,
      });
      const signature = await wallet.signMessage(
        new TextEncoder().encode(challenge.challenge.message),
      );
      const result = await api.createDistributionProject({
        challengeId: challenge.challenge.id,
        signature: signatureToBase64(signature),
        name: form.name.trim(),
        symbol: form.symbol.trim(),
        description: form.description.trim(),
        logoUrl: form.logoUrl.trim(),
        websiteUrl: form.websiteUrl.trim(),
        xHandle: form.xHandle.trim(),
        launchpad: form.launchpad,
        launchpadUrl: form.launchpadUrl.trim(),
        managerMode: form.managerMode,
        creatorWallet: form.managerMode === 'creator' ? wallet.address : undefined,
      });
      setProject(result.project);
      notify({
        variant: 'success',
        title: 'Manager wallet verified',
        description: `The canonical $${result.project.symbol} vault is ready to activate.`,
      });
    } catch (cause) {
      setError(cause?.message ?? 'Could not connect this token.');
    } finally {
      setCreating(false);
    }
  };

  const activate = async () => {
    if (!project || activating) return;
    setActivating(true);
    setError('');
    try {
      const result = await api.activateDistributionProject(
        { slug: project.slug },
        { idempotencyKey: makeIdempotencyKey(`activate-${project.slug}`) },
      );
      notify({
        variant: 'success',
        title: 'Distribution is active',
        description: `$${project.symbol} now has a public ANSEM.TIPS growth vault.`,
      });
      navigate(`/${result.project.slug}`);
    } catch (cause) {
      setError(cause?.message ?? 'Could not activate this vault.');
      setActivating(false);
    }
  };

  return (
    <div className="page dv-page dv-onboarding">
      <Link className="dv-back-link" to="/"><ArrowLeft size={15} /> Distribution network</Link>

      <header className="dv-onboarding-head">
        <p className="dv-eyebrow"><span /> PROJECT ONBOARDING</p>
        <h1>Connect the token.<br /><em>Activate the flywheel.</em></h1>
        <p>One mint gets one canonical growth vault. The launchpad creates the asset; ANSEM.TIPS creates the attention.</p>
      </header>

      {!isLoggedIn ? (
        <OnboardingGate icon={KeyRound} step="01" title="Sign in with X" detail="Your verified X identity becomes the public manager profile for this vault.">
          <button type="button" className="dv-primary-button" onClick={signIn}>Sign in with X <ArrowRight size={16} /></button>
        </OnboardingGate>
      ) : !wallet.isConnected ? (
        <OnboardingGate icon={Wallet} step="02" title="Connect the manager wallet" detail="You will sign a message proving control. No transaction or funds move during verification.">
          <button type="button" className="dv-primary-button" onClick={wallet.connect}>Choose wallet <ArrowRight size={16} /></button>
        </OnboardingGate>
      ) : project ? (
        <ActivationCard
          project={project}
          wallet={wallet.address}
          fee={managed.data?.activationFee ?? '0.2'}
          enabled={managed.data?.enabled !== false}
          error={error}
          activating={activating}
          onActivate={activate}
        />
      ) : (
        <div className="dv-onboarding-grid">
          <form className="dv-connect-form" onSubmit={create}>
            <div className="dv-form-section">
              <div className="dv-form-title"><span>01</span><div><h2>Token identity</h2><p>Read from the mint, presented by the project.</p></div></div>
              <label>Solana mint address<input value={form.mint} onChange={setField('mint')} placeholder="Token mint" required /></label>
              <div className="dv-form-pair">
                <label>Project name<input value={form.name} onChange={setField('name')} placeholder="Example Protocol" required /></label>
                <label>Ticker<input value={form.symbol} onChange={setField('symbol')} placeholder="XYZ" maxLength="12" required /></label>
              </div>
              <label>Description<textarea value={form.description} onChange={setField('description')} placeholder="What is this project building?" rows="3" /></label>
              <div className="dv-form-pair">
                <label>Logo URL<input type="url" value={form.logoUrl} onChange={setField('logoUrl')} placeholder="https://…" /></label>
                <label>Website<input type="url" value={form.websiteUrl} onChange={setField('websiteUrl')} placeholder="https://…" /></label>
              </div>
            </div>

            <div className="dv-form-section">
              <div className="dv-form-title"><span>02</span><div><h2>Launch origin</h2><p>The adapter layer keeps the vault launchpad-agnostic.</p></div></div>
              <div className="dv-form-pair">
                <label>Launchpad<select value={form.launchpad} onChange={setField('launchpad')}><option value="ansem_io">ansem.io</option><option value="pump_fun">Pump.fun</option><option value="stonkfun">Stonkfun</option><option value="meteora">Meteora</option><option value="other">Other</option></select></label>
                <label>Launch URL<input type="url" value={form.launchpadUrl} onChange={setField('launchpadUrl')} placeholder="https://…" /></label>
              </div>
              <label>Project X handle<input value={form.xHandle} onChange={setField('xHandle')} placeholder="@project" /></label>
            </div>

            <div className="dv-form-section">
              <div className="dv-form-title"><span>03</span><div><h2>Management mode</h2><p>The manager controls policy, never arbitrary withdrawals.</p></div></div>
              <div className="dv-mode-picker">
                {[
                  ['creator', 'Creator', 'Original team or fee authority'],
                  ['cto', 'CTO', 'Community takeover operator'],
                  ['community', 'Community', 'Community or multisig manager'],
                ].map(([id, label, detail]) => (
                  <label className={form.managerMode === id ? 'selected' : ''} key={id}><input type="radio" name="managerMode" value={id} checked={form.managerMode === id} onChange={setField('managerMode')} /><ShieldCheck /><strong>{label}</strong><span>{detail}</span></label>
                ))}
              </div>
              <div className="dv-wallet-proof"><BadgeCheck /><div><span>Connected manager wallet</span><strong>{shortAddress(wallet.address)}</strong></div></div>
            </div>

            {wallet.isConnected && !wallet.signMessage && <p className="dv-form-error">This wallet cannot sign verification messages. Choose a wallet with message-signing support.</p>}
            {error && <p className="dv-form-error">{error}</p>}
            <button className="dv-primary-button dv-submit-project" type="submit" disabled={!canCreate}>{creating ? 'Check your wallet…' : 'Verify manager & create vault'} <FileSignature size={17} /></button>
          </form>

          <aside className="dv-onboarding-summary">
            <p className="dv-eyebrow"><span /> WHAT GETS CREATED</p>
            <h2>A public growth treasury.</h2>
            {[['Canonical', 'One vault tied to this mint'], ['Transparent', 'Funding and distributions are public'], ['Non-custodial policy', 'Manager cannot freely withdraw'], ['Composable', 'Launchpad adapters connect later']].map(([title, detail]) => <div key={title}><BadgeCheck /><p><strong>{title}</strong><span>{detail}</span></p></div>)}
            <hr />
            <p><Coins /> Activation fee <strong>{managed.data?.activationFee ?? '0.2'} SOL</strong></p>
            <small>Paid from your ANSEM.TIPS SOL balance only after verification.</small>
          </aside>
        </div>
      )}
    </div>
  );
}

function OnboardingGate({ icon: Icon, step, title, detail, children }) {
  return <section className="dv-onboarding-gate"><span>{step}</span><Icon /><h2>{title}</h2><p>{detail}</p>{children}</section>;
}

function ActivationCard({ project, wallet, fee, enabled, error, activating, onActivate }) {
  return (
    <section className="dv-activation-card">
      <div className="dv-activation-brand"><ProjectMark project={project} large /><p className="dv-eyebrow"><span /> WALLET VERIFIED</p><h2>${project.symbol} is ready.</h2><p>The vault exists in a pending state. Activation makes it public and opens community funding.</p></div>
      <div className="dv-activation-receipt">
        <div><span>Canonical mint</span><strong>{shortAddress(project.mintAddress)}</strong></div>
        <div><span>Manager wallet</span><strong>{shortAddress(wallet)}</strong></div>
        <div><span>Management mode</span><strong>{project.managerMode}</strong></div>
        <div><span>Activation fee</span><strong>{fee} SOL</strong></div>
        <p><CircleDollarSign /> Debited from your ANSEM.TIPS SOL balance. It does not enter campaign capital.</p>
        {error && <p className="dv-form-error">{error}</p>}
        <button className="dv-primary-button" type="button" onClick={onActivate} disabled={!enabled || activating}>{activating ? 'Activating…' : enabled ? 'Pay fee & activate' : 'Activation is not open'} <Rocket size={17} /></button>
        <a className="dv-text-button" href={import.meta.env.VITE_DASHBOARD_URL || 'https://ansem.tips/dashboard'}>Top up SOL <ExternalLink size={14} /></a>
      </div>
    </section>
  );
}
