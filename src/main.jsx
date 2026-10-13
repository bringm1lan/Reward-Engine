import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import { PrivyProvider } from '@privy-io/react-auth';
import { AuthProvider } from './auth';
import { ToastProvider } from './toast';
import { SolanaProvider } from './wallet';
import { ErrorBoundary } from './ErrorBoundary';
import App from './App';
import './styles.css';

const appId = import.meta.env.VITE_PRIVY_APP_ID?.trim();
const root = createRoot(document.getElementById('root'));

const application = (
  <StrictMode>
    <ErrorBoundary>
      <ToastProvider>
        <AuthProvider>
          <SolanaProvider>
            <BrowserRouter><App /></BrowserRouter>
          </SolanaProvider>
        </AuthProvider>
      </ToastProvider>
    </ErrorBoundary>
  </StrictMode>
);

root.render(
  appId ? (
    <PrivyProvider
      appId={appId}
      config={{
        loginMethods: ['twitter'],
        appearance: { theme: 'dark', accentColor: '#64F5A9', logo: '/logo-mark.svg' },
      }}
    >
      {application}
    </PrivyProvider>
  ) : (
    <div className="configuration-error">
      <img src="/logo-mark.svg" alt="" />
      <p>Configuration required</p>
      <h1>Add VITE_PRIVY_APP_ID to start the Distribution V3 website.</h1>
    </div>
  ),
);
