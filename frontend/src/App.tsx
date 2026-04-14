import { useState, useEffect } from 'react';
import { BrowserRouter, Routes, Route, Link, Navigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { api } from '@/lib/api';
import Login from '@/pages/Login';
import Profile from '@/pages/Profile';
import Admin from '@/pages/Admin';
import '@/index.css';

function App() {
  const [user, setUser] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.me().then(setUser).catch(() => setUser(null)).finally(() => setLoading(false));
  }, []);

  const handleLogout = async () => {
    await api.logout();
    setUser(null);
  };

  if (loading) return <div className="min-h-screen flex items-center justify-center text-muted-foreground">Loading...</div>;
  if (!user) return <Login onLogin={setUser} />;

  if (user.status === 'pending') {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <div className="text-center space-y-4 max-w-sm">
          <h1 className="text-2xl font-semibold">Account Pending</h1>
          <p className="text-muted-foreground">Your account is awaiting admin approval. Please check back later.</p>
          <p className="text-sm text-muted-foreground">{user.email}</p>
          <Button variant="ghost" size="sm" onClick={handleLogout}>Logout</Button>
        </div>
      </div>
    );
  }

  if (user.status === 'rejected') {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <div className="text-center space-y-4 max-w-sm">
          <h1 className="text-2xl font-semibold">Account Rejected</h1>
          <p className="text-muted-foreground">Your account registration has been rejected. Contact the administrator for details.</p>
          <p className="text-sm text-muted-foreground">{user.email}</p>
          <Button variant="ghost" size="sm" onClick={handleLogout}>Logout</Button>
        </div>
      </div>
    );
  }

  return (
    <BrowserRouter basename="/_ui">
      <nav className="border-b bg-background px-6 py-3 flex items-center gap-4">
        <span className="font-semibold text-lg">Kiro Proxy</span>
        <Link to="/"><Button variant="ghost" size="sm">Profile</Button></Link>
        {user.role === 'admin' && <Link to="/admin"><Button variant="ghost" size="sm">Admin</Button></Link>}
        <span className="ml-auto text-sm text-muted-foreground">{user.email}</span>
      </nav>
      <Routes>
        <Route path="/" element={<Profile user={user} onLogout={handleLogout} />} />
        {user.role === 'admin' && <Route path="/admin" element={<Admin />} />}
        <Route path="*" element={<Navigate to="/" />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
