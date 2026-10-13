import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ArrowDownLeft,
  ArrowLeft,
  ArrowUpRight,
  BadgeCheck,
  ChartNoAxesCombined,
  CircleDollarSign,
  Copy,
  ExternalLink,
  HandCoins,
  Layers3,
  LockKeyhole,
  Settings2,
  ShieldCheck,
  UsersRound,
} from 'lucide-react';
import { Link, useParams } from 'react-router-dom';
import { useAuth, useXProfile } from './auth';
import { useToast } from './toast';
import { useAsyncResource } from './useAsyncResource';
import { api } from './api';
import {
  allocationRows,
  DistributionState,
  formatSol,
  makeIdempotencyKey,
  PolicyBars,
  ProjectMark,
  shortAddress,
} from './components';
import './distribution.css';

export function DistributionVault() {
  const { slug } = useParams();
  const { isLoggedIn, signIn } = useAuth();
  const profile = useXProfile();
  const { notify } = useToast();
  const fetchProject = useCallback((signal) => api.getDistributionProject({ slug, signal }), [slug]);
  const resource = useAsyncResource(fetchProject, { pollIntervalMs: 30_000 });
  const [amount, setAmount] = useState('');
  const [funding, setFunding] = useState(false);
  const [fundError, setFundError] = useState('');

  const project = resource.data?.project;
  const isManager = Boolean(
    isLoggedIn &&
    profile?.handle &&
    project?.manager?.username &&
    profile.handle.toLowerCase().replace(/^@/, '') === project.manager.username.toLowerCase(),
  );

  useEffect(() => {
    const previous = document.title;
    if (project) document.title = `$${project.symbol} Distribution Vault · Ansem.tips`;
    return () => { document.title = previous; };
  }, [project]);

  const fund = async (event) => {
    event.preventDefault();
    if (!amount.trim() || funding) return;
    setFunding(true);
    setFundError('');
    try {
      const result = await api.contributeToDistributionProject(
        { slug, amount },
        { idempotencyKey: makeIdempotencyKey(`vault-${slug}`) },
      );
      notify({
        variant: 'success',
        title: 'Vault funded',
        description: `${result.amount} SOL joined the ${project.symbol} growth treasury.`,
      });
      setAmount('');
      resource.refresh();
    } catch (error) {
      setFundError(error?.message ?? 'Could not fund this vault.');
    } finally {
      setFunding(false);
    }
  };

  if (resource.error && !project) {
    return (
      <div className="page dv-page">
        <DistributionState title="Vault unavailable" detail={resource.error.message}>
          <Link className="dv-secondary-button" to="/"><ArrowLeft size={16} /> All vaults</Link>
        </DistributionState>
      </div>
    );
  }
  if (!project) {
    return <div className="page dv-page"><DistributionState loading title="Loading distribution vault" /></div>;
  }

  const policy = project.vault?.policy ?? {};
  const ring = allocationRows(policy);
  const ringGradient = `conic-gradient(${ring.map((row, index) => {
    const start = ring.slice(0, index).reduce((sum, item) => sum + item.value, 0) / 100;
    const end = start + row.value / 100;
    return `var(--dv-${row.tone}) ${start}% ${end}%`;
  }).join(', ')})`;

  return (
    <div className="page dv-page dv-detail">
      <Link className="dv-back-link" to="/"><ArrowLeft size={15} /> Distribution network</Link>

      <section className="dv-vault-hero">
        <div className="dv-vault-identity">
          <div className="dv-vault-title-row">
            <ProjectMark project={project} large />
            <div>
              <p className="dv-eyebrow"><span /> VERIFIED DISTRIBUTION VAULT</p>
              <h1>${project.symbol}</h1>
              <p>{project.name}</p>
            </div>
          </div>
          {project.description && <p className="dv-project-description">{project.description}</p>}
          <div className="dv-vault-links">
            <button type="button" onClick={() => navigator.clipboard?.writeText(project.mintAddress)}>
              <Copy size={14} /> {shortAddress(project.mintAddress)}
            </button>
            {project.websiteUrl && <a href={project.websiteUrl} target="_blank" rel="noreferrer noopener">Website <ExternalLink size={14} /></a>}
            {project.xHandle && <a href={`https://x.com/${project.xHandle}`} target="_blank" rel="noreferrer noopener">@{project.xHandle} <ExternalLink size={14} /></a>}
          </div>
        </div>

        <div className="dv-vault-total">
          <span>Available distribution capital</span>
          <strong>{formatSol(project.vault?.balance, 6)} <small>SOL</small></strong>
          <p><i /> {project.status === 'paused' ? 'Funding open · automation paused' : 'Distribution active'}</p>
        </div>

        <div className="dv-allocation-ring" style={{ '--dv-ring': ringGradient }}>
          <div><span>Autopilot</span><strong>100%</strong><small>allocated</small></div>
        </div>
      </section>

      <div className="dv-detail-grid">
        <main>
          <section className="dv-panel dv-capital-panel">
            <header><div><p className="dv-eyebrow"><span /> CAPITAL ROUTER</p><h2>Where every SOL goes.</h2></div><Layers3 /></header>
            <PolicyBars policy={policy} />
            <div className="dv-allocation-cards">
              {ring.map((row) => (
                <article key={row.id}><i className={`tone-${row.tone}`} /><span>{row.label}</span><strong>{row.value / 100}%</strong></article>
              ))}
            </div>
            {isManager && <PolicyEditor project={project} onSaved={resource.refresh} />}
          </section>

          <section className="dv-panel dv-ledger-panel">
            <header><div><p className="dv-eyebrow"><span /> PUBLIC LEDGER</p><h2>Every movement, accounted for.</h2></div><ChartNoAxesCombined /></header>
            {resource.data?.entries?.length ? (
              <div className="dv-ledger-list">
                {resource.data.entries.map((entry) => <LedgerRow key={entry.id} entry={entry} />)}
              </div>
            ) : <p className="dv-empty-copy">Vault activity will appear here.</p>}
          </section>
        </main>

        <aside>
          <section className="dv-panel dv-fund-panel">
            <HandCoins />
            <p className="dv-eyebrow"><span /> COMMUNITY FUNDING</p>
            <h2>Back the distribution.</h2>
            <p>Move SOL from your ANSEM.TIPS balance into this token's public growth treasury.</p>
            {isLoggedIn ? (
              <form onSubmit={fund}>
                <label htmlFor="dv-fund-amount">Amount</label>
                <div className="dv-amount-input"><input id="dv-fund-amount" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} placeholder="0.00" /><span>SOL</span></div>
                {fundError && <p className="dv-form-error">{fundError}</p>}
                <button className="dv-primary-button" type="submit" disabled={!amount.trim() || funding}>{funding ? 'Funding…' : 'Fund this vault'} <ArrowUpRight size={16} /></button>
              </form>
            ) : (
              <button className="dv-primary-button" type="button" onClick={signIn}>Sign in to fund</button>
            )}
            <small><LockKeyhole size={13} /> Vault managers cannot withdraw community funds.</small>
          </section>

          <section className="dv-panel dv-proof-panel">
            <header><h3>Vault proof</h3><ShieldCheck /></header>
            <dl>
              <div><dt>Manager</dt><dd>@{project.manager.username}</dd></div>
              <div><dt>Mode</dt><dd>{project.managerMode}</dd></div>
              <div><dt>Wallet</dt><dd>{shortAddress(project.managerWallet)}</dd></div>
              <div><dt>Launchpad</dt><dd>{project.launchpad.label}</dd></div>
            </dl>
            <p><BadgeCheck size={15} /> Wallet signature verified</p>
          </section>

          <section className="dv-panel dv-contributors-panel">
            <header><h3>Contributors</h3><UsersRound /></header>
            <strong>{resource.data?.stats?.contributorCount ?? 0}</strong>
            <span>{resource.data?.stats?.contributionCount ?? 0} contributions</span>
            <div className="dv-contributor-stack">
              {(resource.data?.contributions ?? []).slice(0, 8).map((row) => row.contributor.avatarUrl ? (
                <img key={row.id} src={row.contributor.avatarUrl} alt={`@${row.contributor.username}`} title={`@${row.contributor.username} · ${row.amount} SOL`} />
              ) : (
                <i key={row.id} title={`@${row.contributor.username} · ${row.amount} SOL`}>{row.contributor.username.slice(0, 1).toUpperCase()}</i>
              ))}
            </div>
          </section>
        </aside>
      </div>
    </div>
  );
}

function LedgerRow({ entry }) {
  const incoming = BigInt(entry.balanceDeltaLamports) > 0n;
  const labels = {
    activation_fee: 'Vault activated',
    contribution: 'Vault funded',
    allocation_update: 'Allocation policy updated',
    creator_fee: 'Creator fee received',
    campaign: 'Campaign funded',
    giveaway: 'Giveaway funded',
    tip: 'Tipping funded',
    tipansem_purchase: 'TIPANSEM purchased',
    protocol_fee: 'Protocol fee',
  };
  const Icon = incoming ? ArrowDownLeft : entry.type === 'allocation_update' ? Settings2 : ArrowUpRight;
  return (
    <div className={`dv-ledger-row ${incoming ? 'incoming' : 'outgoing'}`}>
      <span><Icon /></span>
      <div><strong>{labels[entry.type] ?? entry.type}</strong><small>{new Date(entry.createdAt).toLocaleString()}</small></div>
      <b>{entry.type === 'allocation_update' ? 'POLICY' : `${incoming ? '+' : entry.type === 'activation_fee' ? '' : '−'}${formatSol(entry.amount, 6)} SOL`}</b>
    </div>
  );
}

function PolicyEditor({ project, onSaved }) {
  const { notify } = useToast();
  const initial = useMemo(
    () => Object.fromEntries(allocationRows(project.vault.policy).map((row) => [row.id, String(row.value / 100)])),
    [project.vault.policy],
  );
  const [values, setValues] = useState(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => setValues(initial), [initial]);
  const total = Object.values(values).reduce((sum, value) => sum + (Number(value) || 0), 0);

  const submit = async (event) => {
    event.preventDefault();
    if (total !== 100 || saving) return;
    setSaving(true);
    setError('');
    try {
      await api.updateDistributionPolicy({
        slug: project.slug,
        expectedVersion: project.vault.version,
        ...Object.fromEntries(Object.entries(values).map(([key, value]) => [key, Math.round(Number(value) * 100)])),
      });
      notify({ variant: 'success', title: 'Distribution policy updated', description: 'New incoming capital will follow this allocation.' });
      onSaved();
    } catch (cause) {
      setError(cause?.message ?? 'Could not update the distribution policy.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <details className="dv-policy-editor">
      <summary><Settings2 size={15} /> Manager controls</summary>
      <form onSubmit={submit}>
        <div className="dv-policy-inputs">
          {allocationRows(project.vault.policy).map((row) => (
            <label key={row.id}>{row.label}<span><input type="number" min="0" max="100" step="0.5" value={values[row.id]} onChange={(event) => setValues((current) => ({ ...current, [row.id]: event.target.value }))} />%</span></label>
          ))}
        </div>
        <div className={total === 100 ? 'dv-policy-total valid' : 'dv-policy-total'}><span>Total</span><strong>{total}%</strong></div>
        {error && <p className="dv-form-error">{error}</p>}
        <button className="dv-secondary-button" type="submit" disabled={total !== 100 || saving}>{saving ? 'Saving…' : 'Save allocation'}</button>
      </form>
    </details>
  );
}
