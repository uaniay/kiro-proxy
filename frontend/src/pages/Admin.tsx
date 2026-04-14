import { useState, useEffect } from 'react';
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

type Tab = 'users' | 'usage' | 'pool' | 'accounts';

export default function Admin() {
  const [tab, setTab] = useState<Tab>('users');
  const [users, setUsers] = useState<any[]>([]);
  const [pool, setPool] = useState<any[]>([]);
  const [usage, setUsage] = useState<any[]>([]);
  const [accounts, setAccounts] = useState<any[]>([]);
  const [newPool, setNewPool] = useState({ label: '', refresh_token: '', client_id: '', client_secret: '', sso_region: 'us-east-1' });

  useEffect(() => { loadUsers(); loadPool(); loadUsage(); loadAccounts(); }, []);

  const loadUsers = async () => { try { setUsers((await api.listUsers()).users); } catch {} };
  const loadPool = async () => { try { setPool((await api.listPool()).pool); } catch {} };
  const loadUsage = async () => { try { setUsage((await api.listUsage()).usage); } catch {} };
  const loadAccounts = async () => { try { setAccounts((await api.listAccounts()).accounts); } catch {} };

  const deleteUser = async (id: string) => { if (!confirm('Delete this user?')) return; await api.deleteUser(id); loadUsers(); };
  const approveUser = async (id: string) => { await api.approveUser(id); loadUsers(); };
  const rejectUser = async (id: string) => { if (!confirm('Reject this user?')) return; await api.rejectUser(id); loadUsers(); };
  const addPool = async () => {
    if (!newPool.label || !newPool.refresh_token) return;
    await api.addPool({ label: newPool.label, refresh_token: newPool.refresh_token, client_id: newPool.client_id || undefined, client_secret: newPool.client_secret || undefined, sso_region: newPool.sso_region || undefined });
    setNewPool({ label: '', refresh_token: '', client_id: '', client_secret: '', sso_region: 'us-east-1' });
    loadPool();
    loadAccounts();
  };
  const deletePool = async (id: string) => { if (!confirm('Delete?')) return; await api.deletePool(id); loadPool(); loadAccounts(); };
  const togglePool = async (id: string, enabled: boolean) => { await api.togglePool(id, !enabled); loadPool(); loadAccounts(); };
  const toggleAccount = async (id: string, type: string, enabled: boolean) => {
    await api.toggleAccount(id, type, !enabled);
    loadAccounts();
    if (type === 'pool') loadPool();
  };

  const tabs: { key: Tab; label: string }[] = [
    { key: 'users', label: 'Users' },
    { key: 'usage', label: 'Usage' },
    { key: 'pool', label: 'Token Pool' },
    { key: 'accounts', label: 'Kiro Accounts' },
  ];

  return (
    <div className="max-w-4xl mx-auto p-6 space-y-6">
      <h1 className="text-2xl font-semibold">Admin</h1>

      <div className="flex gap-1 border-b">
        {tabs.map(t => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              tab === t.key
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground'
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === 'users' && (
        <Card>
          <CardHeader>
            <CardTitle>Users ({users.length})</CardTitle>
            <CardDescription>Registered users</CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Email</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Last Login</TableHead>
                  <TableHead></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.map(u => (
                  <TableRow key={u.id}>
                    <TableCell className="font-medium">{u.email}</TableCell>
                    <TableCell className="text-muted-foreground">{u.name}</TableCell>
                    <TableCell><Badge variant={u.role === 'admin' ? 'default' : 'secondary'}>{u.role}</Badge></TableCell>
                    <TableCell><Badge variant={u.status === 'active' ? 'default' : u.status === 'pending' ? 'outline' : 'destructive'}>{u.status}</Badge></TableCell>
                    <TableCell className="text-sm text-muted-foreground">{u.last_login ? new Date(u.last_login).toLocaleDateString() : 'Never'}</TableCell>
                    <TableCell className="flex gap-1">
                      {u.status === 'pending' && <>
                        <Button variant="ghost" size="sm" className="text-green-600" onClick={() => approveUser(u.id)}>Approve</Button>
                        <Button variant="ghost" size="sm" className="text-destructive" onClick={() => rejectUser(u.id)}>Reject</Button>
                      </>}
                      {u.role !== 'admin' && <Button variant="ghost" size="sm" className="text-destructive" onClick={() => deleteUser(u.id)}>Delete</Button>}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {tab === 'usage' && (
        <Card>
          <CardHeader>
            <CardTitle>API Key Usage</CardTitle>
            <CardDescription>Token usage across all users</CardDescription>
          </CardHeader>
          <CardContent>
            {usage.length > 0 ? (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Key</TableHead>
                    <TableHead>User</TableHead>
                    <TableHead className="text-right">Requests</TableHead>
                    <TableHead className="text-right">Input Tokens</TableHead>
                    <TableHead className="text-right">Output Tokens</TableHead>
                    <TableHead>Last Used</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {usage.map(u => (
                    <TableRow key={u.id}>
                      <TableCell><code className="text-sm">{u.key_prefix}</code> {u.name && <span className="text-muted-foreground ml-1">{u.name}</span>}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">{u.user_id.slice(0, 8)}...</TableCell>
                      <TableCell className="text-right font-mono">{formatNumber(u.request_count)}</TableCell>
                      <TableCell className="text-right font-mono">{formatNumber(u.total_input_tokens)}</TableCell>
                      <TableCell className="text-right font-mono">{formatNumber(u.total_output_tokens)}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">{u.last_used ? new Date(u.last_used).toLocaleDateString() : 'Never'}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            ) : (
              <p className="text-sm text-muted-foreground">No usage data yet</p>
            )}
          </CardContent>
        </Card>
      )}

      {tab === 'pool' && (
        <Card>
          <CardHeader>
            <CardTitle>Token Pool ({pool.length})</CardTitle>
            <CardDescription>Admin-managed Kiro tokens for load balancing</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {pool.length > 0 && (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Label</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Region</TableHead>
                    <TableHead>Last Used</TableHead>
                    <TableHead></TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {pool.map(p => (
                    <TableRow key={p.id}>
                      <TableCell className="font-medium">{p.label}</TableCell>
                      <TableCell><Badge variant={p.enabled ? 'default' : 'destructive'}>{p.enabled ? 'Enabled' : 'Disabled'}</Badge></TableCell>
                      <TableCell className="text-muted-foreground">{p.sso_region || '—'}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">{p.last_used ? new Date(p.last_used).toLocaleDateString() : 'Never'}</TableCell>
                      <TableCell className="flex gap-1">
                        <Button variant="ghost" size="sm" onClick={() => togglePool(p.id, p.enabled)}>{p.enabled ? 'Disable' : 'Enable'}</Button>
                        <Button variant="ghost" size="sm" className="text-destructive" onClick={() => deletePool(p.id)}>Delete</Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
            <div className="p-4 border rounded-lg space-y-3">
              <p className="text-sm font-medium">Add Pool Entry</p>
              <div className="grid grid-cols-2 gap-2">
                <Input placeholder="Label *" value={newPool.label} onChange={e => setNewPool({ ...newPool, label: e.target.value })} />
                <Input placeholder="SSO Region" value={newPool.sso_region} onChange={e => setNewPool({ ...newPool, sso_region: e.target.value })} />
                <Input placeholder="Refresh Token *" value={newPool.refresh_token} onChange={e => setNewPool({ ...newPool, refresh_token: e.target.value })} className="col-span-2" />
                <Input placeholder="Client ID" value={newPool.client_id} onChange={e => setNewPool({ ...newPool, client_id: e.target.value })} />
                <Input placeholder="Client Secret" value={newPool.client_secret} onChange={e => setNewPool({ ...newPool, client_secret: e.target.value })} />
              </div>
              <Button onClick={addPool}>Add</Button>
            </div>
          </CardContent>
        </Card>
      )}

      {tab === 'accounts' && (
        <Card>
          <CardHeader>
            <CardTitle>Kiro Accounts ({accounts.length})</CardTitle>
            <CardDescription>All configured Kiro accounts across global, user, and pool</CardDescription>
          </CardHeader>
          <CardContent>
            {accounts.length > 0 ? (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Type</TableHead>
                    <TableHead>Label</TableHead>
                    <TableHead>Region</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Has Token</TableHead>
                    <TableHead>Last Used</TableHead>
                    <TableHead></TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {accounts.map(a => (
                    <TableRow key={`${a.type}-${a.id}`}>
                      <TableCell>
                        <Badge variant={a.type === 'global' ? 'default' : a.type === 'user' ? 'secondary' : 'outline'}>
                          {a.type}
                        </Badge>
                      </TableCell>
                      <TableCell className="font-medium">{a.label}</TableCell>
                      <TableCell className="text-muted-foreground">{a.region || '—'}</TableCell>
                      <TableCell>
                        <Badge variant={a.enabled ? 'default' : 'destructive'}>
                          {a.enabled ? 'Enabled' : 'Disabled'}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">{a.has_token ? 'Yes' : 'No'}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {a.last_used ? new Date(a.last_used).toLocaleDateString() : '—'}
                      </TableCell>
                      <TableCell>
                        <Button variant="ghost" size="sm" onClick={() => toggleAccount(a.id, a.type, a.enabled)}>
                          {a.enabled ? 'Disable' : 'Enable'}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            ) : (
              <p className="text-sm text-muted-foreground">No Kiro accounts configured</p>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
