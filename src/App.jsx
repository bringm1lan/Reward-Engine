import { Navigate, Route, Routes, useParams } from 'react-router-dom';
import { SiteLayout } from './SiteLayout';
import { DistributionDirectory } from './DistributionDirectory';
import { DistributionCreate } from './DistributionCreate';
import { DistributionVault } from './DistributionVault';

export default function App() {
  return (
    <Routes>
      <Route element={<SiteLayout />}>
        <Route index element={<DistributionDirectory />} />
        <Route path="new" element={<DistributionCreate />} />
        <Route path=":slug" element={<DistributionVault />} />
        <Route path="distribution" element={<Navigate to="/" replace />} />
        <Route path="distribution/new" element={<Navigate to="/new" replace />} />
        <Route path="distribution/:slug" element={<LegacyVaultRedirect />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

function LegacyVaultRedirect() {
  const { slug } = useParams();
  return <Navigate to={`/${slug}`} replace />;
}
