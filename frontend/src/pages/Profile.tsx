import { useState, useEffect } from 'react';
import { api } from '../lib/api';

export default function Profile({ user, onLogout }: { user: any; onLogout: () => void }) {
  const [keys, setKeys] = useState<any[]>([]);
  const [newKeyName, setNewKeyName] = useState('');
  const [createdKey, setCreatedKey] = useState('');
  const [kiroStatus, setKiroStatus] = useState<any>(null);
  const [setupData, setSetupData] = useState<any>(null);
  const [ssoUrl, setSsoUrl] = useState('');
  const [ssoRegion, setSsoRegion] = useState('us-east-1');
  const [polling, setPolling] = useState(false);

  useEffect(() => { loadKeys(); loadKiroStatus(); }, []);

  const loadKeys = async () => {
    try { const data = await api.listKeys(); setKeys(data.keys); } catch {}
  };
  const loadKiroStatus = async () => {
    try { setKiroStatus(await api.kiroStatus()); } catch {}
  };

  const createKey = async () => {
    try {
      const data = await api.createKey(newKeyName);
      setCreatedKey(data.key);
      setNewKeyName('');
      loadKeys();
    } catch {}
  };

  const deleteKey = async (id: string) => {
    if (!confirm('Delete this API key?')) return;
    await api.deleteKey(id);
    loadKeys();
  };

  const startKiroSetup = async () => {
    try {
      const data = await api.kiroSetup(ssoUrl || undefined, ssoRegion || undefined);
      setSetupData(data);
      setPolling(true);
      pollDevice(data.device_code, data.interval);
    } catch {}
  };

  const pollDevice = async (deviceCode: string, interval: number) => {
    const poll = async () => {
      try {
        const result = await api.kiroPoll(deviceCode);
        if (result.status === 'success') {
          setPolling(false);
          setSetupData(null);
          loadKiroStatus();
          return;
        }
        if (result.status === 'pending' || result.status === 'slow_down') {
          setTimeout(poll, (interval || 5) * 1000);
        }
      } catch { setPolling(false); }
    };
    setTimeout(poll, interval * 1000);
  };

  const deleteToken = async () => {
    if (!confirm('Remove your Kiro token?')) return;
    await api.kiroDelete();
    loadKiroStatus();
  };

  return (
    <div className="max-w-2xl mx-auto p-6">
      <div className="flex justify-between items-center mb-8">
        <div>
          <h1 className="text-2xl font-semibold">{user.email}</h1>
          <span className="text-sm px-2 py-0.5 bg-blue-100 text-blue-700 rounded">{user.role}</span>
        </div>
        <button onClick={onLogout} className="text-sm text-gray-500 hover:text-red-500">Logout</button>
      </div>

      {/* API Keys */}
      <section className="mb-8">
        <h2 className="text-lg font-medium mb-3">API Keys</h2>
        {createdKey && (
          <div className="mb-3 p-3 bg-green-50 border border-green-200 rounded text-sm">
            <p className="font-medium text-green-800">New key created (copy now, shown once):</p>
            <code className="block mt-1 break-all text-green-900 bg-green-100 p-2 rounded">{createdKey}</code>
            <button onClick={() => setCreatedKey('')} className="mt-2 text-xs text-green-600 hover:underline">Dismiss</button>
          </div>
        )}
        <div className="flex gap-2 mb-3">
          <input placeholder="Key name (optional)" value={newKeyName} onChange={e => setNewKeyName(e.target.value)}
            className="flex-1 px-3 py-1.5 border rounded text-sm" />
          <button onClick={createKey} className="px-4 py-1.5 bg-blue-600 text-white rounded text-sm hover:bg-blue-700">Create</button>
        </div>
        <div className="space-y-2">
          {keys.map(k => (
            <div key={k.id} className="flex justify-between items-center p-3 bg-gray-50 rounded border">
              <div>
                <code className="text-sm">{k.key_prefix}...</code>
                {k.name && <span className="ml-2 text-sm text-gray-500">{k.name}</span>}
                <span className="ml-2 text-xs text-gray-400">{k.last_used ? `Used ${k.last_used}` : 'Never used'}</span>
              </div>
              <button onClick={() => deleteKey(k.id)} className="text-sm text-red-500 hover:text-red-700">Delete</button>
            </div>
          ))}
          {keys.length === 0 && <p className="text-sm text-gray-400">No API keys yet</p>}
        </div>
      </section>

      {/* Kiro Token */}
      <section>
        <h2 className="text-lg font-medium mb-3">Kiro Token</h2>
        {kiroStatus?.has_token && !kiroStatus?.expired ? (
          <div className="p-3 bg-green-50 border border-green-200 rounded">
            <p className="text-sm text-green-800">Token active {kiroStatus.sso_region && `(${kiroStatus.sso_region})`}</p>
            <button onClick={deleteToken} className="mt-2 text-sm text-red-500 hover:underline">Remove token</button>
          </div>
        ) : setupData ? (
          <div className="p-4 bg-yellow-50 border border-yellow-200 rounded">
            <p className="text-sm mb-2">Open this URL and enter the code:</p>
            <a href={setupData.verification_uri_complete} target="_blank" rel="noreferrer"
              className="text-blue-600 hover:underline text-sm break-all">{setupData.verification_uri_complete}</a>
            <p className="mt-2 text-lg font-mono font-bold">{setupData.user_code}</p>
            {polling && <p className="mt-2 text-sm text-yellow-700 animate-pulse">Waiting for authorization...</p>}
          </div>
        ) : (
          <div className="space-y-3">
            <div className="flex gap-2">
              <input placeholder="SSO Start URL (optional)" value={ssoUrl} onChange={e => setSsoUrl(e.target.value)}
                className="flex-1 px-3 py-1.5 border rounded text-sm" />
              <input placeholder="Region" value={ssoRegion} onChange={e => setSsoRegion(e.target.value)}
                className="w-32 px-3 py-1.5 border rounded text-sm" />
            </div>
            <button onClick={startKiroSetup} className="px-4 py-1.5 bg-blue-600 text-white rounded text-sm hover:bg-blue-700">
              Bind Kiro Token
            </button>
            {kiroStatus?.has_token && kiroStatus?.expired && (
              <p className="text-sm text-red-500">Token expired. Re-bind to refresh.</p>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
