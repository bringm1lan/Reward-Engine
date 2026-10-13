import { ArrowUpRight, Box, LoaderCircle } from 'lucide-react';

// Shared presentation primitives for the standalone Distribution V3 website.

export const allocationRows = (policy = {}) => [
  { id: 'campaignBps', label: 'Campaigns', value: Number(policy.campaignBps ?? 0), tone: 'lime' },
  { id: 'giveawayBps', label: 'Giveaways', value: Number(policy.giveawayBps ?? 0), tone: 'aqua' },
  { id: 'tipBps', label: 'Tipping', value: Number(policy.tipBps ?? 0), tone: 'violet' },
  { id: 'tipansemPurchaseBps', label: 'TIPANSEM', value: Number(policy.tipansemPurchaseBps ?? 0), tone: 'gold' },
  { id: 'reserveBps', label: 'Reserve', value: Number(policy.reserveBps ?? 0), tone: 'slate' },
];

export const formatSol = (value, maximumFractionDigits = 3) =>
  Number(value ?? 0).toLocaleString(undefined, { maximumFractionDigits });

export const shortAddress = (value) => value ? `${value.slice(0, 5)}…${value.slice(-5)}` : '—';

export const makeIdempotencyKey = (scope) =>
  `${scope}:${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`;

export function DistributionState({ icon: Icon = Box, title, detail, loading = false, children }) {
  return (
    <div className="dv-state">
      <span>{loading ? <LoaderCircle className="dv-spin" /> : <Icon />}</span>
      <h2>{title}</h2>
      {detail && <p>{detail}</p>}
      {children}
    </div>
  );
}

export function ProjectMark({ project, large = false }) {
  if (project?.logoUrl) {
    return <img className={large ? 'dv-project-mark large' : 'dv-project-mark'} src={project.logoUrl} alt="" />;
  }
  return (
    <span className={large ? 'dv-project-mark large fallback' : 'dv-project-mark fallback'}>
      {(project?.symbol ?? '?').slice(0, 2)}
    </span>
  );
}

export function PolicyBars({ policy }) {
  const rows = allocationRows(policy);
  return (
    <div className="dv-policy-bars">
      <div className="dv-policy-track" aria-label="Distribution allocation">
        {rows.map((row) => (
          <span
            key={row.id}
            className={`tone-${row.tone}`}
            style={{ width: `${row.value / 100}%` }}
            title={`${row.label}: ${row.value / 100}%`}
          />
        ))}
      </div>
      <div className="dv-policy-legend">
        {rows.map((row) => (
          <span key={row.id}><i className={`tone-${row.tone}`} /> {row.label} <strong>{row.value / 100}%</strong></span>
        ))}
      </div>
    </div>
  );
}

export function LaunchpadLabel({ launchpad }) {
  return (
    <span className="dv-launchpad">
      {launchpad?.label ?? 'Launchpad'}
      {launchpad?.url && <ArrowUpRight size={13} aria-hidden="true" />}
    </span>
  );
}
