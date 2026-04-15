import { useState, useEffect } from 'react';
import { BrowserRouter, Routes, Route, Link, Navigate } from 'react-router-dom';
import { Sun, Moon, User, Shield, LogOut } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { api } from '@/lib/api';
import { ThemeProvider, useTheme } from '@/lib/theme';
import Login from '@/pages/Login';
import Profile from '@/pages/Profile';
import Admin from '@/pages/Admin';
import '@/index.css';

function ThemeToggle() {
  const { theme, toggle } = useTheme();
  return (
    <button
      onClick={toggle}
      className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors cursor-pointer"
      aria-label="Toggle theme"
    >
      {theme === 'light' ? <Moon className="w-4 h-4" /> : <Sun className="w-4 h-4" />}
    </button>
  );
}

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
          <div className="flex items-center justify-center gap-2">
            <ThemeToggle />
            <Button variant="ghost" size="sm" onClick={handleLogout}>Logout</Button>
          </div>
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
          <div className="flex items-center justify-center gap-2">
            <ThemeToggle />
            <Button variant="ghost" size="sm" onClick={handleLogout}>Logout</Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <BrowserRouter basename="/_ui">
      <nav className="sticky top-0 z-50 border-b border-border/50 bg-card backdrop-blur-xl px-6 py-3 flex items-center gap-1">
        <span className="font-semibold text-lg text-primary mr-4">Kiro Proxy</span>
        <Link to="/">
          <Button variant="ghost" size="sm" className="gap-1.5 cursor-pointer">
            <User className="w-4 h-4" />
            Profile
          </Button>
        </Link>
        {user.role === 'admin' && (
          <Link to="/admin">
            <Button variant="ghost" size="sm" className="gap-1.5 cursor-pointer">
              <Shield className="w-4 h-4" />
              Admin
            </Button>
          </Link>
        )}
        <div className="ml-auto flex items-center gap-2">
          <span className="text-sm text-muted-foreground">{user.email}</span>
          <ThemeToggle />
          <button
            onClick={handleLogout}
            className="p-2 rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors cursor-pointer"
            aria-label="Logout"
          >
            <LogOut className="w-4 h-4" />
          </button>
        </div>
      </nav>
      <Routes>
        <Route path="/" element={<Profile user={user} onLogout={handleLogout} />} />
        {user.role === 'admin' && <Route path="/admin" element={<Admin />} />}
        <Route path="*" element={<Navigate to="/" />} />
      </Routes>
    </BrowserRouter>
  );
}

export default function Root() {
  return (
    <ThemeProvider>
      <App />
    </ThemeProvider>
  );
}
