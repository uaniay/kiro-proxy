import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { api } from '@/lib/api';

function formatNumber(n: number) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toString();
}

export default function Profile({ user, onLogout }: { user: any; onLogout: () => void }) {
  const navigate = useNavigate();
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
        if (result.status === 'success') { setPolling(false); setSetupData(null); loadKiroStatus(); return; }
        if (result.status === 'pending' || result.status === 'slow_down') setTimeout(poll, (interval || 5) * 1000);
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
    <div className="max-w-3xl mx-auto p-6 space-y-6">
      <div className="flex justify-between items-center">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-semibold">{user.email}</h1>
          <Badge variant={user.role === 'admin' ? 'default' : 'secondary'}>{user.role}</Badge>
        </div>
        <Button variant="ghost" size="sm" onClick={onLogout}>Logout</Button>
      </div>

      {/* API Keys */}
      <Card>
        <CardHeader>
          <CardTitle>API Keys</CardTitle>
          <CardDescription>Manage your API keys for accessing the proxy</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {createdKey && (
            <div className="p-3 bg-green-50 dark:bg-green-950 border border-green-200 dark:border-green-800 rounded-lg">
              <p className="text-sm font-medium text-green-800 dark:text-green-200">New key created (copy now, shown once):</p>
              <code className="block mt-1 text-sm break-all bg-green-100 dark:bg-green-900 p-2 rounded">{createdKey}</code>
              <Button variant="ghost" size="sm" className="mt-2" onClick={() => setCreatedKey('')}>Dismiss</Button>
            </div>
          )}
          <div className="flex gap-2">
            <Input placeholder="Key name (optional)" value={newKeyName} onChange={e => setNewKeyName(e.target.value)} />
            <Button onClick={createKey}>Create Key</Button>
          </div>
          {keys.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Prefix</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead className="text-right">Requests</TableHead>
                  <TableHead className="text-right">Input Tokens</TableHead>
                  <TableHead className="text-right">Output Tokens</TableHead>
                  <TableHead>Last Used</TableHead>
                  <TableHead></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {keys.map(k => (
                  <TableRow key={k.id}>
                    <TableCell><code className="text-sm">{k.key_prefix}...</code></TableCell>
                    <TableCell className="text-muted-foreground">{k.name || '—'}</TableCell>
                    <TableCell className="text-right font-mono">{formatNumber(k.request_count || 0)}</TableCell>
                    <TableCell className="text-right font-mono">{formatNumber(k.total_input_tokens || 0)}</TableCell>
                    <TableCell className="text-right font-mono">{formatNumber(k.total_output_tokens || 0)}</TableCell>
                    <TableCell className="text-muted-foreground text-sm">{k.last_used ? new Date(k.last_used).toLocaleDateString() : 'Never'}</TableCell>
                    <TableCell className="flex gap-1">
                      {user.role === 'admin' && (
                        <Button variant="ghost" size="sm" onClick={() => navigate(`/admin?tab=conversations&api_key_id=${k.id}`)}>Logs</Button>
                      )}
                      <Button variant="ghost" size="sm" className="text-destructive" onClick={() => deleteKey(k.id)}>Delete</Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <p className="text-sm text-muted-foreground">No API keys yet</p>
          )}
        </CardContent>
      </Card>

      {/* Kiro Token */}
      <Card>
        <CardHeader>
          <CardTitle>Kiro Token</CardTitle>
          <CardDescription>Bind your AWS SSO credentials for Kiro API access</CardDescription>
        </CardHeader>
        <CardContent>
          {kiroStatus?.has_token && !kiroStatus?.expired ? (
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Badge variant="default" className="bg-green-600">Active</Badge>
                {kiroStatus.sso_region && <span className="text-sm text-muted-foreground">{kiroStatus.sso_region}</span>}
              </div>
              <Button variant="destructive" size="sm" onClick={deleteToken}>Remove</Button>
            </div>
          ) : setupData ? (
            <div className="space-y-3 p-4 bg-yellow-50 dark:bg-yellow-950 border border-yellow-200 dark:border-yellow-800 rounded-lg">
              <p className="text-sm">Open this URL and enter the code:</p>
              <a href={setupData.verification_uri_complete} target="_blank" rel="noreferrer" className="text-primary hover:underline text-sm break-all">{setupData.verification_uri_complete}</a>
              <p className="text-2xl font-mono font-bold tracking-wider">{setupData.user_code}</p>
              {polling && <p className="text-sm text-yellow-700 dark:text-yellow-300 animate-pulse">Waiting for authorization...</p>}
            </div>
          ) : (
            <div className="space-y-3">
              <div className="flex gap-2">
                <Input placeholder="SSO Start URL (optional)" value={ssoUrl} onChange={e => setSsoUrl(e.target.value)} />
                <Input placeholder="Region" value={ssoRegion} onChange={e => setSsoRegion(e.target.value)} className="w-36" />
              </div>
              <Button onClick={startKiroSetup}>Bind Kiro Token</Button>
              {kiroStatus?.has_token && kiroStatus?.expired && (
                <p className="text-sm text-destructive">Token expired. Re-bind to refresh.</p>
              )}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
