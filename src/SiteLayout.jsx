import { ExternalLink, LogIn, LogOut, Menu, Network, Plus, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link, NavLink, Outlet, useLocation } from 'react-router-dom';
import { useAuth } from './auth';

export function SiteLayout() {
  const { isLoggedIn, isSigningIn, profile, signIn, signOut } = useAuth();
  const [menuOpen, setMenuOpen] = useState(false);
  const location = useLocation();
  useEffect(() => setMenuOpen(false), [location.pathname]);

  return (
    <div className="site-shell">
      <header className="site-header">
        <Link className="site-brand" to="/">
          <img src="/logo-mark.svg" alt="" />
          <span><strong>ANSEM.TIPS</strong><small>DISTRIBUTION</small></span>
        </Link>

        <button className="site-menu-button" type="button" onClick={() => setMenuOpen((value) => !value)} aria-label="Toggle navigation">
          {menuOpen ? <X /> : <Menu />}
        </button>

        <div className={menuOpen ? 'site-navigation open' : 'site-navigation'}>
          <nav aria-label="Distribution navigation">
            <NavLink to="/" end><Network size={15} /> Vault network</NavLink>
            <NavLink to="/new"><Plus size={15} /> Connect token</NavLink>
            <a href={import.meta.env.VITE_DASHBOARD_URL || 'https://ansem.tips/dashboard'}>ANSEM.TIPS <ExternalLink size={14} /></a>
          </nav>
          {isLoggedIn ? (
            <div className="site-account">
              {profile?.avatarUrl ? <img src={profile.avatarUrl} alt="" /> : <span>{(profile?.displayName || 'A')[0]}</span>}
              <div><strong>{profile?.handle ? `@${profile.handle}` : profile?.displayName}</strong><small>Distribution account</small></div>
              <button type="button" onClick={signOut} aria-label="Sign out"><LogOut /></button>
            </div>
          ) : (
            <button className="site-sign-in" type="button" onClick={signIn} disabled={isSigningIn}>
              <LogIn size={15} /> {isSigningIn ? 'Opening X…' : 'Sign in with X'}
            </button>
          )}
        </div>
      </header>
      <main className="site-main"><Outlet /></main>
      <footer className="site-footer">
        <span>ANSEM.TIPS V3</span>
        <p>The token generates the budget. ANSEM.TIPS turns it into attention.</p>
      </footer>
    </div>
  );
}
