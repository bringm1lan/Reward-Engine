import { getAccessToken, getIdentityToken } from '@privy-io/react-auth';

const configuredBase = import.meta.env.VITE_API_BASE_URL?.trim();
const API_BASE = configuredBase
  ? `${configuredBase.replace(/\/+$/, '')}${/\/api$/i.test(configuredBase.replace(/\/+$/, '')) ? '' : '/api'}`
  : '';

export class ApiError extends Error {
  constructor(message, { status = 0, code = 'REQUEST_FAILED', cause } = {}) {
    super(message, { cause });
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

const pathFor = (path) => `${API_BASE}${path}`;

async function request(path, { method = 'GET', body, headers, signal, auth = true } = {}) {
  if (!API_BASE) {
    throw new ApiError('VITE_API_BASE_URL is not configured for this website.', {
      code: 'API_NOT_CONFIGURED',
    });
  }

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 15_000);
  const abort = () => controller.abort();
  signal?.addEventListener('abort', abort, { once: true });

  try {
    let accessToken = null;
    let identityToken = null;
    if (auth) {
      accessToken = await getAccessToken().catch(() => null);
      if (!accessToken) {
        throw new ApiError('Sign in with X to continue.', { status: 401, code: 'SIGN_IN_REQUIRED' });
      }
      identityToken = await getIdentityToken().catch(() => null);
    }

    const response = await fetch(pathFor(path), {
      method,
      signal: controller.signal,
      headers: {
        Accept: 'application/json',
        ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
        ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
        ...(identityToken ? { 'Privy-Id-Token': identityToken } : {}),
        ...headers,
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });

    const payload = await response.json().catch(() => null);
    if (!response.ok) {
      throw new ApiError(
        payload?.error?.message || payload?.message || `Request failed (${response.status}).`,
        {
          status: response.status,
          code: payload?.error?.code || payload?.code || 'REQUEST_FAILED',
        },
      );
    }
    return payload;
  } catch (error) {
    if (error?.name === 'AbortError') {
      throw new ApiError('The request timed out. Try again.', { code: 'REQUEST_TIMEOUT', cause: error });
    }
    if (error instanceof ApiError) throw error;
    throw new ApiError('Could not reach the distribution service.', { cause: error });
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener('abort', abort);
  }
}

const encode = (value) => encodeURIComponent(value ?? '');

export const api = {
  getDistributionProjects: ({ limit = 24, ...options } = {}) =>
    request(`/distribution/projects?limit=${encode(limit)}`, { auth: false, ...options }),

  getDistributionProject: ({ slug, ...options }) =>
    request(`/distribution/projects/${encode(slug)}`, { auth: false, ...options }),

  getManagedDistributionProjects: (options) =>
    request('/distribution/manage/projects', options),

  createDistributionChallenge: (body, options) =>
    request('/distribution/manager-challenges', { method: 'POST', body, ...options }),

  createDistributionProject: (body, options) =>
    request('/distribution/projects', { method: 'POST', body, ...options }),

  activateDistributionProject: ({ slug }, { idempotencyKey, ...options } = {}) =>
    request(`/distribution/projects/${encode(slug)}/activate`, {
      method: 'POST',
      body: {},
      headers: { 'Idempotency-Key': idempotencyKey },
      ...options,
    }),

  contributeToDistributionProject: ({ slug, amount }, { idempotencyKey, ...options } = {}) =>
    request(`/distribution/projects/${encode(slug)}/contributions`, {
      method: 'POST',
      body: { amount },
      headers: { 'Idempotency-Key': idempotencyKey },
      ...options,
    }),

  updateDistributionPolicy: ({ slug, ...body }, options) =>
    request(`/distribution/projects/${encode(slug)}/policy`, {
      method: 'PATCH',
      body,
      ...options,
    }),
};
