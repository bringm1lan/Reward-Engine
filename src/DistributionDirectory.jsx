import { useCallback, useEffect } from 'react';
import { ArrowRight, Coins, Orbit, Plus, Radar, Route, Sparkles } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useAsyncResource } from './useAsyncResource';
import { api } from './api';
import {
  DistributionState,
  formatSol,
  LaunchpadLabel,
  PolicyBars,
  ProjectMark,
  shortAddress,
} from './components';
import './distribution.css';

export function DistributionDirectory() {
  const fetchProjects = useCallback((signal) => api.getDistributionProjects({ signal, limit: 48 }), []);
  const resource = useAsyncResource(fetchProjects, { pollIntervalMs: 30_000 });

  useEffect(() => {
    const previous = document.title;
    document.title = 'Distribution Vaults · Ansem.tips';
    return () => { document.title = previous; };
  }, []);

  const data = resource.data;

  return (
    <div className="page dv-page dv-directory">
      <section className="dv-directory-hero">
        <div className="dv-orbit-art" aria-hidden="true">
          <span className="orbit orbit-a"><i /></span>
          <span className="orbit orbit-b"><i /></span>
          <span className="orbit orbit-c"><i /></span>
          <div className="orbit-core"><Route /></div>
        </div>
        <div className="dv-directory-copy">
          <p className="dv-eyebrow"><span /> ANSEM.TIPS V3</p>
          <h1>Creator fees become <em>distribution.</em></h1>
          <p className="dv-lede">
            One transparent growth vault per token. Fund campaigns, giveaways and tipping from one public treasury.
          </p>
          <div className="dv-hero-actions">
            <Link className="dv-primary-button" to="/new"><Plus size={17} /> Connect a token</Link>
            <a className="dv-text-button" href="#vault-network">Explore vaults <ArrowRight size={16} /></a>
          </div>
        </div>

        <div className="dv-network-stats">
          <article><Coins /><span>Vault balance</span><strong>{formatSol(data?.stats?.vaultBalance)} SOL</strong></article>
          <article><Sparkles /><span>Lifetime funded</span><strong>{formatSol(data?.stats?.lifetimeFunded)} SOL</strong></article>
          <article><Radar /><span>Connected projects</span><strong>{data?.stats?.projectCount ?? '—'}</strong></article>
        </div>
      </section>

      <section id="vault-network" className="dv-vault-network">
        <header className="dv-section-head">
          <div><p className="dv-eyebrow"><span /> LIVE NETWORK</p><h2>Self-funding growth, in public.</h2></div>
          <p>Anyone can fund a vault. Only its verified manager can configure distribution.</p>
        </header>

        {resource.error && !data ? (
          <DistributionState title="Vaults are temporarily unavailable" detail={resource.error.message}>
            <button type="button" onClick={resource.reload}>Try again</button>
          </DistributionState>
        ) : resource.isLoading && !data ? (
          <div className="dv-project-grid" aria-busy="true">
            {[0, 1, 2].map((item) => <div key={item} className="dv-project-card skeleton" />)}
          </div>
        ) : data?.projects?.length ? (
          <div className="dv-project-grid">
            {data.projects.map((project) => <ProjectCard key={project.id} project={project} />)}
          </div>
        ) : (
          <DistributionState icon={Orbit} title="The first vault is waiting" detail="Connect a launched Solana token and make its distribution treasury public.">
            <Link className="dv-primary-button" to="/new">Create the first vault</Link>
          </DistributionState>
        )}
      </section>

      <section className="dv-loop">
        {['Trade', 'Creator fees', 'Distribution vault', 'Campaigns + tips', 'Attention'].map((step, index) => (
          <div key={step}><span>{String(index + 1).padStart(2, '0')}</span><strong>{step}</strong>{index < 4 && <ArrowRight />}</div>
        ))}
      </section>
    </div>
  );
}

function ProjectCard({ project }) {
  const content = (
    <>
      <div className="dv-project-card-head">
        <ProjectMark project={project} />
        <div><strong>${project.symbol}</strong><span>{project.name}</span></div>
        <span className={`dv-status ${project.status}`}><i /> {project.status}</span>
      </div>
      <div className="dv-project-balance">
        <span>Distribution vault</span>
        <strong>{formatSol(project.vault?.balance)} <small>SOL</small></strong>
        <p>{formatSol(project.vault?.lifetimeFunded)} SOL lifetime funding</p>
      </div>
      <PolicyBars policy={project.vault?.policy} />
      <footer>
        <LaunchpadLabel launchpad={project.launchpad} />
        <span>{project.managerMode} · {shortAddress(project.managerWallet)}</span>
      </footer>
    </>
  );

  return <Link className="dv-project-card" to={`/${project.slug}`}>{content}</Link>;
}
