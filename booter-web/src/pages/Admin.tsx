import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { AmbientBackground } from '../components/AmbientBackground';
import { GlassCard } from '../components/GlassCard';
import { CompanionsPanel } from '../components/CompanionsPanel';
import { SessionsPanel } from '../components/SessionsPanel';
import { useAuth } from '../context/AuthContext';
import { fetchClient } from '../utils/fetchClient';

import { QRCodeSVG } from 'qrcode.react';



export default function Admin() {
  const navigate = useNavigate();
  const { token } = useAuth();

  
  // User Management State
  const [users, setUsers] = useState<string[]>([]);
  const [newUserEmail, setNewUserEmail] = useState('');
  const [userLoading, setUserLoading] = useState(true);

  // Auto Shutdown State
  const [autoShutdownMinutes, setAutoShutdownMinutes] = useState<number>(0);
  const [savingAutoShutdown, setSavingAutoShutdown] = useState(false);

  // Boot Restrictions State
  const [cooldownMinutes, setCooldownMinutes] = useState<number>(0);
  const [forbiddenTime, setForbiddenTime] = useState<string>('');
  const [savingRestrictions, setSavingRestrictions] = useState(false);

  // Mijia State
  const [mijiaStatus, setMijiaStatus] = useState<'idle' | 'polling' | 'logged_in'>('idle');
  const [qrUrl, setQrUrl] = useState('');
  const [lpUrl, setLpUrl] = useState('');
  const [currentDid, setCurrentDid] = useState<string | null>(null);
  const [devices, setDevices] = useState<any[]>([]);
  const [mijiaLoading, setMijiaLoading] = useState(true);
  const [savingDid, setSavingDid] = useState(false);

  useEffect(() => {

    
    const fetchUsers = async () => {
      try {
        const data = await fetchClient('/api/v1/admin/users', {
          headers: { 'Authorization': `Bearer ${token}` }
        });
        if (data.success && data.users) {
          setUsers(data.users);
        }
      } catch (err) {
        console.error("Failed to load users", err);
      } finally {
        setUserLoading(false);
      }
    };
    
    const fetchMijiaStatus = async () => {
      try {
        const data = await fetchClient('/api/v1/admin/mijia/status', {
          headers: { 'Authorization': `Bearer ${token}` }
        });
        if (data.success && data.is_logged_in) {
          setMijiaStatus('logged_in');
          setCurrentDid(data.current_did);
        } else {
          setMijiaStatus('idle');
        }
      } catch (err) {
        console.error(err);
      } finally {
        setMijiaLoading(false);
      }
    };

    const fetchAutoShutdown = async () => {
      try {
        const data = await fetchClient('/api/v1/admin/autoshutdown', {
          headers: { 'Authorization': `Bearer ${token}` }
        });
        if (data.success) {
          setAutoShutdownMinutes(data.minutes);
        }
      } catch (err) {
        console.error(err);
      }
    };

    const fetchBootRestrictions = async () => {
      try {
        const data = await fetchClient('/api/v1/admin/boot_restrictions', {
          headers: { 'Authorization': `Bearer ${token}` }
        });
        if (data.success) {
          setCooldownMinutes(data.cooldown_minutes);
          setForbiddenTime(data.forbidden_time);
        }
      } catch (err) {
        console.error(err);
      }
    };


    fetchUsers();
    fetchMijiaStatus();
    fetchAutoShutdown();
    fetchBootRestrictions();
  }, [token]);

  const fetchDevices = async () => {
    try {
      const data = await fetchClient('/api/v1/admin/mijia/devices', {
        headers: { 'Authorization': `Bearer ${token}` }
      });
      if (data.success) {
        setDevices(data.devices);
      }
    } catch (err) {
      console.error(err);
    }
  };

  useEffect(() => {
    if (mijiaStatus === 'logged_in') {
      fetchDevices();
    }
  }, [mijiaStatus]);

  const startMijiaLogin = async () => {
    setMijiaLoading(true);
    try {
      const data = await fetchClient('/api/v1/admin/mijia/qr/start', {
        headers: { 'Authorization': `Bearer ${token}` }
      });
      if (data.success) {
        setQrUrl(data.qr_url);
        setLpUrl(data.lp_url);
        setMijiaStatus('polling');
      }
    } catch (err) {
      console.error(err);
    } finally {
      setMijiaLoading(false);
    }
  };

  useEffect(() => {
    let pollingTimer: any = null;
    if (mijiaStatus === 'polling' && lpUrl) {
      const poll = async () => {
        try {
          const data = await fetchClient('/api/v1/admin/mijia/qr/poll', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              'Authorization': `Bearer ${token}`
            },
            body: JSON.stringify({ lp_url: lpUrl })
          });
          if (data.success) {
            setMijiaStatus('logged_in');
          } else {
            pollingTimer = setTimeout(poll, 3000);
          }
        } catch (err) {
          pollingTimer = setTimeout(poll, 3000);
        }
      };
      poll();
    }
    return () => clearTimeout(pollingTimer);
  }, [mijiaStatus, lpUrl, token]);

  const saveSelectedDevice = async (did: string) => {
    setSavingDid(true);
    setCurrentDid(did);
    try {
      await fetchClient('/api/v1/admin/mijia/devices/select', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`
        },
        body: JSON.stringify({ did })
      });
    } catch (err) {
      console.error(err);
    } finally {
      setSavingDid(false);
    }
  };

  const handleAddUser = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newUserEmail) return;
    try {
      const data = await fetchClient('/api/v1/admin/users', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`
        },
        body: JSON.stringify({ email: newUserEmail })
      });
      if (data.success) {
        setUsers([...users, newUserEmail]);
        setNewUserEmail('');
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleDeleteUser = async (email: string) => {
    try {
      const data = await fetchClient(`/api/v1/admin/users/${encodeURIComponent(email)}`, {
        method: 'DELETE',
        headers: { 'Authorization': `Bearer ${token}` }
      });
      if (data.success) {
        setUsers(users.filter(u => u !== email));
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSaveAutoShutdown = async (e: React.FormEvent) => {
    e.preventDefault();
    setSavingAutoShutdown(true);
    try {
      await fetchClient('/api/v1/admin/autoshutdown', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`
        },
        body: JSON.stringify({ minutes: autoShutdownMinutes })
      });
    } catch (err) {
      console.error(err);
    } finally {
      setSavingAutoShutdown(false);
    }
  };

  const handleSaveBootRestrictions = async (e: React.FormEvent) => {
    e.preventDefault();
    setSavingRestrictions(true);
    try {
      await fetchClient('/api/v1/admin/boot_restrictions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`
        },
        body: JSON.stringify({ cooldown_minutes: Number(cooldownMinutes), forbidden_time: forbiddenTime })
      });
    } catch (err) {
      console.error(err);
    } finally {
      setSavingRestrictions(false);
    }
  };

  return (
    <div className="dark">
      <div className="min-h-screen p-4 md:p-12 bg-slate-50 dark:bg-slate-900 transition-colors duration-500 relative overflow-hidden">
        
        <AmbientBackground color="bg-rose-500" />

        <div className="max-w-7xl mx-auto relative z-10">
          <header className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 md:gap-0 mb-10 md:mb-16">
            <h1 className="text-3xl md:text-4xl font-extrabold text-slate-900 dark:text-white tracking-tight transition-colors">管理面板</h1>
            <button 
              onClick={() => navigate('/dashboard')}
              className="text-sm bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700 px-6 py-2.5 rounded-full transition-colors border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-300 font-medium"
            >
              返回控制台
            </button>
          </header>

          <div className="grid grid-cols-1 gap-8 md:gap-12">
            
            <GlassCard 
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              className="p-6 md:p-10"
            >
              <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-4 transition-colors">米家配置</h2>
              <p className="text-slate-500 dark:text-slate-400 mb-8 transition-colors">绑定米家账号以获取智能设备的控制权。</p>
              
              <div className="p-8 border border-slate-200 dark:border-slate-700 rounded-2xl flex flex-col items-center justify-center bg-white dark:bg-[#1e1e1e] transition-colors min-h-[300px]">
                {mijiaLoading ? (
                  <span className="text-slate-500">加载中...</span>
                ) : mijiaStatus === 'idle' ? (
                  <div className="text-center">
                    <p className="text-slate-500 mb-6">您尚未登录米家账号</p>
                    <button 
                      onClick={startMijiaLogin}
                      className="bg-indigo-600 hover:bg-indigo-700 text-white font-medium px-8 py-3 rounded-xl transition-colors"
                    >
                      生成登录二维码
                    </button>
                  </div>
                ) : mijiaStatus === 'polling' ? (
                  <div className="text-center flex flex-col items-center">
                    <div className="bg-white p-4 rounded-xl mb-4">
                      <QRCodeSVG value={qrUrl} size={200} />
                    </div>
                    <p className="text-indigo-500 font-medium animate-pulse">请打开【米家 APP】扫描二维码...</p>
                  </div>
                ) : (
                  <div className="w-full max-w-lg">
                    <div className="flex items-center gap-3 mb-8 justify-center">
                      <div className="w-3 h-3 rounded-full bg-emerald-500 shadow-[0_0_10px_rgba(16,185,129,0.5)]"></div>
                      <span className="text-emerald-500 font-bold">米家已成功授权</span>
                    </div>
                    
                    <div className="flex flex-col gap-2">
                      <label className="text-slate-700 dark:text-slate-300 font-medium text-sm">选择物理唤醒终端 (智能插座):</label>
                      <select 
                        value={currentDid || ''}
                        onChange={(e) => saveSelectedDevice(e.target.value)}
                        disabled={savingDid}
                        className="bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-xl px-4 py-3 text-slate-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-indigo-500 transition-all"
                      >
                        <option value="" disabled>-- 请选择一个设备 --</option>
                        {devices.map(d => (
                          <option key={d.did} value={d.did}>{d.name} ({d.model})</option>
                        ))}
                      </select>
                      {savingDid && <span className="text-indigo-500 text-sm mt-2">保存中...</span>}
                    </div>
                    
                    <div className="mt-8 text-center">
                      <button 
                        onClick={startMijiaLogin}
                        className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 text-sm underline transition-colors"
                      >
                        重新授权账号
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </GlassCard>

            <GlassCard className="p-6 md:p-10 mb-8 md:mb-12">
              <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-2 transition-colors">自动关机策略</h2>
              <p className="text-slate-500 dark:text-slate-400 mb-8 transition-colors">
                全局参数。设置系统在没有任何探针活跃时，多长时间后下发关机指令（填 0 代表禁用自动关机）。
              </p>
              
              <form onSubmit={handleSaveAutoShutdown} className="flex flex-col md:flex-row md:items-center gap-4 mb-8">
                <div className="flex gap-4">
                  <input
                    type="number"
                    step="1"
                    required
                    value={autoShutdownMinutes}
                    onChange={(e) => setAutoShutdownMinutes(Math.floor(Number(e.target.value)))}
                    placeholder="分钟"
                    className="w-full md:w-32 bg-white/50 dark:bg-slate-900/50 border border-slate-200 dark:border-slate-700 rounded-xl px-4 py-3 text-slate-900 dark:text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
                  />
                  <span className="flex items-center text-slate-600 dark:text-slate-400 font-medium">分钟</span>
                </div>
                <button
                  type="submit"
                  disabled={savingAutoShutdown}
                  className="bg-indigo-600 hover:bg-indigo-700 disabled:bg-indigo-400 text-white font-medium px-8 py-3 rounded-xl transition-colors ml-auto"
                >
                  {savingAutoShutdown ? '保存中...' : '保存配置'}
                </button>
              </form>
            </GlassCard>

            {/* Boot Restrictions Panel */}
            <GlassCard className="p-6 md:p-10">
              <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-2 transition-colors">开机访问限制（仅针对普通用户）</h2>
              <p className="text-slate-500 dark:text-slate-400 mb-8 transition-colors">设置普通用户开机冷却时间及禁用时段（管理员不受限制）。</p>
              
              <form onSubmit={handleSaveBootRestrictions} className="flex flex-col gap-6">
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">冷却时间 (分钟)</label>
                  <input
                    type="number"
                    step="1"
                    min="0"
                    value={cooldownMinutes}
                    onChange={(e) => setCooldownMinutes(Math.floor(Number(e.target.value)))}
                    className="w-full bg-white/50 dark:bg-slate-900/50 border border-slate-200 dark:border-slate-700 rounded-xl px-4 py-3 text-slate-900 dark:text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
                    placeholder="0 为不限制"
                  />
                  <p className="mt-2 text-sm text-slate-500 dark:text-slate-400">距离上次开机小于此时间则拒绝开机。</p>
                </div>

                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">禁用时段 (格式: HH:MM-HH:MM)</label>
                  <input
                    type="text"
                    value={forbiddenTime}
                    onChange={(e) => setForbiddenTime(e.target.value)}
                    className="w-full bg-white/50 dark:bg-slate-900/50 border border-slate-200 dark:border-slate-700 rounded-xl px-4 py-3 text-slate-900 dark:text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
                    placeholder="例: 23:00-08:00"
                  />
                  <p className="mt-2 text-sm text-slate-500 dark:text-slate-400">留空为不限制。</p>
                </div>

                <button
                  type="submit"
                  disabled={savingRestrictions}
                  className="bg-indigo-600 hover:bg-indigo-700 disabled:opacity-50 text-white font-medium py-3 px-6 rounded-xl transition-colors self-start mt-2"
                >
                  {savingRestrictions ? '保存中...' : '保存配置'}
                </button>
              </form>
            </GlassCard>


            {/* User Management Panel */}
            <GlassCard className="p-6 md:p-10 mb-8 md:mb-12">
              <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-2 transition-colors">用户访问白名单</h2>
              <p className="text-slate-500 dark:text-slate-400 mb-8 transition-colors">添加邮箱至白名单，允许其通过 OTP 登录普通控制台。</p>
              
              <form onSubmit={handleAddUser} className="flex flex-col md:flex-row gap-4 mb-8">
                <input
                  type="email"
                  required
                  value={newUserEmail}
                  onChange={(e) => setNewUserEmail(e.target.value)}
                  placeholder="user@example.com"
                  className="flex-1 w-full bg-white/50 dark:bg-slate-900/50 border border-slate-200 dark:border-slate-700 rounded-xl px-4 py-3 text-slate-900 dark:text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
                />
                <button
                  type="submit"
                  className="w-full md:w-auto bg-indigo-600 hover:bg-indigo-700 text-white font-medium px-8 py-3 rounded-xl transition-colors"
                >
                  添加用户
                </button>
              </form>

              <div className="border border-slate-200 dark:border-slate-700 rounded-xl overflow-hidden">
                {userLoading ? (
                  <div className="p-4 text-center text-slate-500">加载中...</div>
                ) : users.length === 0 ? (
                  <div className="p-4 text-center text-slate-500">暂无白名单用户</div>
                ) : (
                  <ul className="divide-y divide-slate-200 dark:divide-slate-700">
                    {users.map(u => (
                      <li key={u} className="flex justify-between items-center p-4 bg-white dark:bg-[#1e1e1e]">
                        <span className="text-slate-700 dark:text-slate-300 font-medium">{u}</span>
                        <button
                          onClick={() => handleDeleteUser(u)}
                          className="text-rose-500 hover:text-rose-700 text-sm font-medium transition-colors px-3 py-1 bg-rose-50 dark:bg-rose-950/30 rounded-lg"
                        >
                          移除
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </GlassCard>
            
            <CompanionsPanel token={token} />
            <SessionsPanel />
          </div>
        </div>
      </div>
    </div>
  );
}
