const BASE = '/_ui/api';

async function request(path: string, options?: RequestInit) {
  const res = await fetch(`${BASE}${path}`, {
    credentials: 'include',
    headers: { 'Content-Type': 'application/json', ...options?.headers },
    ...options,
  });
  const data = await res.json();
  if (!res.ok) throw new Error(data?.error?.message || res.statusText);
  return data;
}

export const api = {
  register: (email: string, name: string, password: string) =>
    request('/auth/register', { method: 'POST', body: JSON.stringify({ email, name, password }) }),
  login: (email: string, password: string) =>
    request('/auth/login', { method: 'POST', body: JSON.stringify({ email, password }) }),
  logout: () => request('/auth/logout', { method: 'POST' }),
  me: () => request('/auth/me'),

  listKeys: () => request('/keys'),
  createKey: (name: string) => request('/keys', { method: 'POST', body: JSON.stringify({ name }) }),
  deleteKey: (id: string) => request(`/keys/${id}`, { method: 'DELETE' }),

  kiroSetup: (sso_start_url?: string, sso_region?: string) =>
    request('/kiro/setup', { method: 'POST', body: JSON.stringify({ sso_start_url, sso_region }) }),
  kiroPoll: (device_code: string) =>
    request('/kiro/poll', { method: 'POST', body: JSON.stringify({ device_code }) }),
  kiroStatus: () => request('/kiro/status'),
  kiroDelete: () => request('/kiro/token', { method: 'DELETE' }),

  listUsers: () => request('/admin/users'),
  deleteUser: (id: string) => request(`/admin/users/${id}`, { method: 'DELETE' }),
  approveUser: (id: string) => request(`/admin/users/${id}/approve`, { method: 'POST' }),
  rejectUser: (id: string) => request(`/admin/users/${id}/reject`, { method: 'POST' }),
  listPool: () => request('/admin/pool'),
  addPool: (data: { label: string; refresh_token: string; client_id?: string; client_secret?: string; sso_region?: string }) =>
    request('/admin/pool', { method: 'POST', body: JSON.stringify(data) }),
  deletePool: (id: string) => request(`/admin/pool/${id}`, { method: 'DELETE' }),
  togglePool: (id: string, enabled: boolean) =>
    request(`/admin/pool/${id}`, { method: 'PATCH', body: JSON.stringify({ enabled }) }),
  listUsage: () => request('/admin/usage'),
  listAccounts: () => request('/admin/accounts'),
  toggleAccount: (id: string, type: string, enabled: boolean) =>
    request(`/admin/accounts/${id}`, { method: 'PATCH', body: JSON.stringify({ type, enabled }) }),
};
