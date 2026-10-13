import { createContext, useCallback, useContext, useMemo } from 'react';
import { useLoginWithOAuth, usePrivy } from '@privy-io/react-auth';

const AuthContext = createContext(null);

export function AuthProvider({ children }) {
  const { ready, authenticated, user, logout } = usePrivy();
  const { initOAuth, loading } = useLoginWithOAuth();

  const xAccount = (user?.linkedAccounts ?? []).find((account) => account.type === 'twitter_oauth');
  const profile = useMemo(() => user ? {
    handle: xAccount?.username ?? null,
    displayName: xAccount?.name || xAccount?.username || 'Your account',
    avatarUrl: xAccount?.profilePictureUrl ?? null,
    accountId: xAccount?.subject ?? null,
  } : null, [user, xAccount?.username, xAccount?.name, xAccount?.profilePictureUrl, xAccount?.subject]);

  const signIn = useCallback(() => initOAuth({ provider: 'twitter' }), [initOAuth]);
  const signOut = useCallback(() => logout(), [logout]);
  const value = useMemo(() => ({
    isReady: ready,
    isLoggedIn: authenticated,
    isSigningIn: loading,
    profile,
    signIn,
    signOut,
  }), [authenticated, loading, profile, ready, signIn, signOut]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) throw new Error('useAuth must be used within AuthProvider');
  return context;
}

export function useXProfile() {
  return useAuth().profile;
}
