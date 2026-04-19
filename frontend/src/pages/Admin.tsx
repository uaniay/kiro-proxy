import { useState, useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';
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

type Tab = 'users' | 'usage' | 'pool' | 'accounts' | 'conversations';

export default function Admin() {
  const [searchParams] = useSearchParams();
  const initialTab = (searchParams.get('tab') as Tab) || 'users';
  const initialKeyFilter = searchParams.get('key_prefix') || '';
  const [tab, setTab] = useState<Tab>(initialTab);
  const [users, setUsers] = useState<any[]>([]);
  const [pool, setPool] = useState<any[]>([]);
  const [usage, setUsage] = useState<any[]>([]);
  const [accounts, setAccounts] = useState<any[]>([]);
  const [selectedUsers, setSelectedUsers] = useState<Set<string>>(new Set());
  const [newPool, setNewPool] = useState({ label: '', sso_region: 'us-east-1' });
  const [poolDevice, setPoolDevice] = useState<{ pool_id: string; device_code: string; user_code: string; verification_uri: string; verification_uri_complete?: string } | null>(null);
  const [poolPolling, setPoolPolling] = useState(false);
  const [conversations, setConversations] = useState<any[]>([]);
  const [convTotal, setConvTotal] = useState(0);
  const [convOffset, setConvOffset] = useState(0);
  const [convSearch, setConvSearch] = useState('');
  const [convKeyFilter, setConvKeyFilter] = useState(initialKeyFilter);
  const [selectedConv, setSelectedConv] = useState<any>(null);
  const [convLoading, setConvLoading] = useState(false);

  useEffect(() => {
    if (tab === 'users') loadUsers();
    else if (tab === 'usage') loadUsage();
    else if (tab === 'pool') loadPool();
    else if (tab === 'accounts') loadAccounts();
    else if (tab === 'conversations') loadConversations();
  }, [tab]);

  const loadUsers = async () => { try { setUsers((await api.listUsers()).users); } catch {} };
  const loadPool = async () => { try { setPool((await api.listPool()).pool); } catch {} };
  const loadUsage = async () => { try { setUsage((await api.listUsage()).usage); } catch {} };
  const loadAccounts = async () => { try { setAccounts((await api.listAccounts()).accounts); } catch {} };
  const loadConversations = async (offset = convOffset, search = convSearch, keyId = convKeyFilter) => {
    setConvLoading(true);
    try {
      const res = await api.listConversations({ offset, limit: 10, search: search || undefined, key_prefix: keyId || undefined });
      setConversations(res.conversations);
      setConvTotal(res.total);
      setConvOffset(offset);
    } catch {} finally { setConvLoading(false); }
  };
  const viewConversation = async (id: string) => {
    try { setSelectedConv(await api.getConversation(id)); } catch {}
  };
  const deleteConversation = async (id: string) => {
    if (!confirm('Delete this conversation log?')) return;
    try { await api.deleteConversation(id); loadConversations(); setSelectedConv(null); } catch {}
  };

  const deleteUser = async (id: string) => { if (!confirm('Delete this user?')) return; await api.deleteUser(id); loadUsers(); };
  const approveUser = async (id: string) => { await api.approveUser(id); loadUsers(); };
  const rejectUser = async (id: string) => { if (!confirm('Reject this user?')) return; await api.rejectUser(id); loadUsers(); };

  const togglePoolAllowed = async (id: string, current: boolean) => {
    await api.togglePoolAllowed(id, !current);
    loadUsers();
  };

  const startPoolSetup = async () => {
    if (!newPool.label) return;
    try {
      const res = await api.poolSetup(newPool.label, newPool.sso_region || undefined);
      setPoolDevice({ pool_id: res.pool_id, device_code: res.device_code, user_code: res.user_code, verification_uri: res.verification_uri, verification_uri_complete: res.verification_uri_complete });
      setPoolPolling(true);
      pollPoolDevice(res.pool_id, res.device_code, res.interval || 5);
    } catch (e: any) {
      alert(e.message || 'Setup failed');
    }
  };

  const pollPoolDevice = async (poolId: string, deviceCode: string, interval: number) => {
    const poll = async () => {
      try {
        const res = await api.poolPoll(poolId, deviceCode);
        if (res.status === 'success') {
          setPoolDevice(null);
          setPoolPolling(false);
          setNewPool({ label: '', sso_region: 'us-east-1' });
          loadPool();
          loadAccounts();
          return;
        }
        if (res.status === 'pending' || res.status === 'slow_down') {
          setTimeout(poll, (res.status === 'slow_down' ? interval + 5 : interval) * 1000);
          return;
        }
      } catch {
        setPoolDevice(null);
        setPoolPolling(false);
      }
    };
    setTimeout(poll, interval * 1000);
  };
  const deletePool = async (id: string) => { if (!confirm('Delete?')) return; await api.deletePool(id); loadPool(); loadAccounts(); };
  const togglePool = async (id: string, enabled: boolean) => { await api.togglePool(id, !enabled); loadPool(); loadAccounts(); };
  const toggleAccount = async (id: string, type: string, enabled: boolean) => {
    await api.toggleAccount(id, type, !enabled);
    loadAccounts();
    if (type === 'pool') loadPool();
  };
  const toggleUserSelect = (id: string) => {
    setSelectedUsers(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };
  const shareSelected = async (shared: boolean) => {
    if (selectedUsers.size === 0) return;
    await api.shareUsers([...selectedUsers], shared);
    setSelectedUsers(new Set());
    loadAccounts();
  };

  const tabs: { key: Tab; label: string }[] = [
    { key: 'users', label: 'Users' },
    { key: 'usage', label: 'Usage' },
    { key: 'pool', label: 'Token Pool' },
    { key: 'accounts', label: 'Kiro Accounts' },
    { key: 'conversations', label: 'Conversations' },
  ];

  return (
    <div className="max-w-5xl mx-auto p-6 space-y-6">
      <h1 className="text-2xl font-semibold text-primary">Admin</h1>

      <div className="flex gap-1 border-b border-border/50">
        {tabs.map(t => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors cursor-pointer ${
              tab === t.key
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground hover:border-border'
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === 'users' && (
        <Card className="backdrop-blur-xl border-border/50 shadow-sm">
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
                  <TableHead>Pool</TableHead>
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
                    <TableCell>
                      {u.role === 'admin' ? (
                        <Badge variant="default">Allowed</Badge>
                      ) : (
                        <Button variant="ghost" size="sm" onClick={() => togglePoolAllowed(u.id, u.pool_allowed)}>
                          <Badge variant={u.pool_allowed ? 'default' : 'secondary'}>{u.pool_allowed ? 'Allowed' : 'Denied'}</Badge>
                        </Button>
                      )}
                    </TableCell>
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
        <Card className="backdrop-blur-xl border-border/50 shadow-sm">
          <CardHeader>
            <CardTitle>API Key Usage</CardTitle>
            <CardDescription>Token usage across all users</CardDescription>
          </CardHeader>
          <CardContent>
            {usage.length > 0 ? (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>User</TableHead>
                    <TableHead>Key</TableHead>
                    <TableHead className="text-right">Requests</TableHead>
                    <TableHead className="text-right">Input Tokens</TableHead>
                    <TableHead className="text-right">Output Tokens</TableHead>
                    <TableHead>Last Used</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {usage.map(u => (
                    <TableRow key={u.id}>
                      <TableCell className="text-sm">{u.user_email} <span className="text-muted-foreground">({u.user_name})</span></TableCell>
                      <TableCell>
                        <code
                          className="text-sm cursor-pointer hover:text-foreground"
                          title="Click to copy"
                          onClick={() => { navigator.clipboard.writeText(u.key_prefix); }}
                        >{u.key_prefix}</code>
                        {u.name && <span className="text-muted-foreground ml-1">{u.name}</span>}
                      </TableCell>
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
        <Card className="backdrop-blur-xl border-border/50 shadow-sm">
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
              {poolDevice ? (
                <div className="space-y-2">
                  <p className="text-sm">Open the link below and enter the code:</p>
                  <div className="flex items-center gap-2">
                    <code className="text-lg font-bold">{poolDevice.user_code}</code>
                    <a href={poolDevice.verification_uri_complete || poolDevice.verification_uri} target="_blank" rel="noopener noreferrer" className="text-sm text-blue-600 underline">
                      {poolDevice.verification_uri}
                    </a>
                  </div>
                  {poolPolling && <p className="text-sm text-muted-foreground">Waiting for authorization...</p>}
                </div>
              ) : (
                <>
                  <div className="grid grid-cols-2 gap-2">
                    <Input placeholder="Label *" value={newPool.label} onChange={e => setNewPool({ ...newPool, label: e.target.value })} />
                    <Input placeholder="SSO Region" value={newPool.sso_region} onChange={e => setNewPool({ ...newPool, sso_region: e.target.value })} />
                  </div>
                  <Button onClick={startPoolSetup} disabled={!newPool.label}>Authorize</Button>
                </>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {tab === 'accounts' && (
        <Card className="backdrop-blur-xl border-border/50 shadow-sm">
          <CardHeader>
            <CardTitle>Kiro Accounts ({accounts.length})</CardTitle>
            <CardDescription>All configured Kiro accounts across global, user, and pool</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {selectedUsers.size > 0 && (
              <div className="flex items-center gap-2">
                <span className="text-sm text-muted-foreground">{selectedUsers.size} selected</span>
                <Button size="sm" onClick={() => shareSelected(true)}>Share to Pool</Button>
                <Button size="sm" variant="outline" onClick={() => shareSelected(false)}>Unshare</Button>
                <Button size="sm" variant="ghost" onClick={() => setSelectedUsers(new Set())}>Clear</Button>
              </div>
            )}
            {accounts.length > 0 ? (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="w-8"></TableHead>
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
                        {a.type === 'user' && (
                          <input type="checkbox" checked={selectedUsers.has(a.id)} onChange={() => toggleUserSelect(a.id)} />
                        )}
                      </TableCell>
                      <TableCell>
                        <Badge variant={a.type === 'global' ? 'default' : a.type === 'user' ? 'secondary' : 'outline'}>
                          {a.type}
                        </Badge>
                      </TableCell>
                      <TableCell className="font-medium">
                        {a.label}
                        {a.shared && <Badge variant="outline" className="ml-2">Shared</Badge>}
                      </TableCell>
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

      {tab === 'conversations' && (
        <Card className="backdrop-blur-xl border-border/50 shadow-sm">
          <CardHeader>
            <CardTitle>Conversations</CardTitle>
            <CardDescription>API request/response logs (requires ENABLE_CONVERSATION_LOG=true)</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex gap-2">
              <Input
                placeholder="Search content..."
                value={convSearch}
                onChange={e => setConvSearch(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') { setConvOffset(0); loadConversations(0, convSearch, convKeyFilter); } }}
                className="max-w-xs"
              />
              <Input
                placeholder="Filter by API Key ID..."
                value={convKeyFilter}
                onChange={e => setConvKeyFilter(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') { setConvOffset(0); loadConversations(0, convSearch, convKeyFilter); } }}
                className="max-w-xs"
              />
              <Button variant="outline" size="sm" onClick={() => { setConvOffset(0); loadConversations(0, convSearch, convKeyFilter); }}>
                Search
              </Button>
            </div>

            {convLoading ? (
              <p className="text-sm text-muted-foreground">Loading...</p>
            ) : conversations.length > 0 ? (
              <>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Time</TableHead>
                      <TableHead>User</TableHead>
                      <TableHead>Model</TableHead>
                      <TableHead>API</TableHead>
                      <TableHead>Stream</TableHead>
                      <TableHead>Tokens</TableHead>
                      <TableHead>Duration</TableHead>
                      <TableHead></TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {conversations.map(c => (
                      <TableRow key={c.id} className="cursor-pointer" onClick={() => viewConversation(c.id)}>
                        <TableCell className="text-sm text-muted-foreground whitespace-nowrap">
                          {new Date(c.created_at).toLocaleString()}
                        </TableCell>
                        <TableCell className="text-sm">{c.user_email || c.user_id?.slice(0, 8)}</TableCell>
                        <TableCell><Badge variant="outline">{c.model}</Badge></TableCell>
                        <TableCell><Badge variant="secondary">{c.api_type}</Badge></TableCell>
                        <TableCell className="text-sm">{c.is_stream ? 'Yes' : 'No'}</TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {formatNumber(c.input_tokens)} / {formatNumber(c.output_tokens)}
                        </TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {c.duration_ms ? `${c.duration_ms}ms` : '—'}
                        </TableCell>
                        <TableCell>
                          <Button variant="ghost" size="sm" onClick={e => { e.stopPropagation(); deleteConversation(c.id); }}>
                            Delete
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
                <div className="flex items-center justify-between text-sm text-muted-foreground">
                  <span>{convOffset + 1}–{Math.min(convOffset + 10, convTotal)} of {convTotal}</span>
                  <div className="flex gap-2">
                    <Button variant="outline" size="sm" disabled={convOffset === 0}
                      onClick={() => loadConversations(Math.max(0, convOffset - 10))}>
                      Prev
                    </Button>
                    <Button variant="outline" size="sm" disabled={convOffset + 10 >= convTotal}
                      onClick={() => loadConversations(convOffset + 10)}>
                      Next
                    </Button>
                  </div>
                </div>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">No conversation logs found</p>
            )}

            {selectedConv && (
              <Card className="mt-4 border-primary/30">
                <CardHeader className="pb-2">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-base">
                      {selectedConv.model} — {new Date(selectedConv.created_at).toLocaleString()}
                    </CardTitle>
                    <Button variant="ghost" size="sm" onClick={() => setSelectedConv(null)}>Close</Button>
                  </div>
                  <CardDescription>
                    {selectedConv.api_type} | {selectedConv.is_stream ? 'streaming' : 'non-streaming'} | {selectedConv.duration_ms}ms | Key: {selectedConv.key_prefix || '—'}
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  {selectedConv.request_headers && (
                    <details>
                      <summary className="text-sm font-medium cursor-pointer text-muted-foreground">Request Headers</summary>
                      <pre className="mt-1 p-2 bg-muted/50 rounded text-xs overflow-auto max-h-40">
                        {JSON.stringify(JSON.parse(selectedConv.request_headers), null, 2)}
                      </pre>
                    </details>
                  )}
                  <details open>
                    <summary className="text-sm font-medium cursor-pointer">Request Body</summary>
                    <pre className="mt-1 p-2 bg-muted/50 rounded text-xs overflow-auto max-h-96">
                      {(() => { try { return JSON.stringify(JSON.parse(selectedConv.request_body), null, 2); } catch { return selectedConv.request_body; } })()}
                    </pre>
                  </details>
                  {selectedConv.response_body && (
                    <details open>
                      <summary className="text-sm font-medium cursor-pointer">Response Body</summary>
                      <pre className="mt-1 p-2 bg-muted/50 rounded text-xs overflow-auto max-h-96">
                        {(() => { try { return JSON.stringify(JSON.parse(selectedConv.response_body), null, 2); } catch { return selectedConv.response_body; } })()}
                      </pre>
                    </details>
                  )}
                </CardContent>
              </Card>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
