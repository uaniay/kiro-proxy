import { useState, useEffect } from 'react';
import { api } from '../lib/api';

export default function Admin() {
  const [users, setUsers] = useState<any[]>([]);
  const [pool, setPool] = useState<any[]>([]);
  const [newPool, setNewPool] = useState({ label: '', refresh_token: '', client_id: '', client_secret: '', sso_region: 'us-east-1' });

  useEffect(() => { loadUsers(); loadPool(); }, []);

  const loadUsers = async () => {
    try { setUsers((await api.listUsers()).users); } catch {}
  };
  const loadPool = async () => {
    try { setPool((await api.listPool()).pool); } catch {}
  };

  const deleteUser = async (id: string) => {
    if (!confirm('Delete this user?')) return;
    await api.deleteUser(id);
    loadUsers();
  };

  const addPool = async () => {
    if (!newPool.label || !newPool.refresh_token) return;
    await api.addPool({
      label: newPool.label,
      refresh_token: newPool.refresh_token,
      client_id: newPool.client_id || undefined,
      client_secret: newPool.client_secret || undefined,
      sso_region: newPool.sso_region || undefined,
    });
    setNewPool({ label: '', refresh_token: '', client_id: '', client_secret: '', sso_region: 'us-east-1' });
    loadPool();
  };

  const deletePool = async (id: string) => {
    if (!confirm('Delete this pool entry?')) return;
    await api.deletePool(id);
    loadPool();
  };

  const togglePool = async (id: string, enabled: boolean) => {
    await api.togglePool(id, !enabled);
    loadPool();
  };

  return (
    <div className="max-w-3xl mx-auto p-6">
      <h1 className="text-2xl font-semibold mb-6">Admin</h1>

      {/* Users */}
      <section className="mb-8">
        <h2 className="text-lg font-medium mb-3">Users ({users.length})</h2>
        <div className="space-y-2">
          {users.map(u => (
            <div key={u.id} className="flex justify-between items-center p-3 bg-gray-50 rounded border">
              <div>
                <span className="font-medium">{u.email}</span>
                <span className="ml-2 text-xs px-1.5 py-0.5 bg-blue-100 text-blue-700 rounded">{u.role}</span>
                <span className="ml-2 text-xs text-gray-400">{u.name}</span>
              </div>
              {u.role !== 'admin' && (
                <button onClick={() => deleteUser(u.id)} className="text-sm text-red-500 hover:text-red-700">Delete</button>
              )}
            </div>
          ))}
        </div>
      </section>

      {/* Token Pool */}
      <section>
        <h2 className="text-lg font-medium mb-3">Token Pool ({pool.length})</h2>
        <div className="space-y-2 mb-4">
          {pool.map(p => (
            <div key={p.id} className="flex justify-between items-center p-3 bg-gray-50 rounded border">
              <div>
                <span className="font-medium">{p.label}</span>
                <span className={`ml-2 text-xs px-1.5 py-0.5 rounded ${p.enabled ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}`}>
                  {p.enabled ? 'enabled' : 'disabled'}
                </span>
                {p.sso_region && <span className="ml-2 text-xs text-gray-400">{p.sso_region}</span>}
                {p.token_expiry && <span className="ml-2 text-xs text-gray-400">expires {p.token_expiry}</span>}
              </div>
              <div className="flex gap-2">
                <button onClick={() => togglePool(p.id, p.enabled)} className="text-sm text-blue-500 hover:underline">
                  {p.enabled ? 'Disable' : 'Enable'}
                </button>
                <button onClick={() => deletePool(p.id)} className="text-sm text-red-500 hover:text-red-700">Delete</button>
              </div>
            </div>
          ))}
          {pool.length === 0 && <p className="text-sm text-gray-400">No pool entries</p>}
        </div>

        <div className="p-4 bg-gray-50 rounded border space-y-2">
          <h3 className="text-sm font-medium">Add Pool Entry</h3>
          <div className="grid grid-cols-2 gap-2">
            <input placeholder="Label *" value={newPool.label} onChange={e => setNewPool({ ...newPool, label: e.target.value })}
              className="px-3 py-1.5 border rounded text-sm" />
            <input placeholder="SSO Region" value={newPool.sso_region} onChange={e => setNewPool({ ...newPool, sso_region: e.target.value })}
              className="px-3 py-1.5 border rounded text-sm" />
            <input placeholder="Refresh Token *" value={newPool.refresh_token} onChange={e => setNewPool({ ...newPool, refresh_token: e.target.value })}
              className="col-span-2 px-3 py-1.5 border rounded text-sm" />
            <input placeholder="Client ID" value={newPool.client_id} onChange={e => setNewPool({ ...newPool, client_id: e.target.value })}
              className="px-3 py-1.5 border rounded text-sm" />
            <input placeholder="Client Secret" value={newPool.client_secret} onChange={e => setNewPool({ ...newPool, client_secret: e.target.value })}
              className="px-3 py-1.5 border rounded text-sm" />
          </div>
          <button onClick={addPool} className="px-4 py-1.5 bg-blue-600 text-white rounded text-sm hover:bg-blue-700">Add</button>
        </div>
      </section>
    </div>
  );
}
