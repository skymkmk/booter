import { useState, useEffect, useCallback } from "react";
import { LogsPanel } from '../components/LogsPanel';
import { AdminDelayShutdown } from '../components/AdminDelayShutdown';
import { AmbientBackground } from "../components/AmbientBackground";
import { GlassCard } from "../components/GlassCard";
import { useAuth } from "../context/AuthContext";
import { useWebTransport } from "../hooks/useWebTransport";
import { toast } from "sonner";
import { fetchClient } from "../utils/fetchClient";
export default function Dashboard() {
  const { logout, role, token } = useAuth();
  const [isOnline, setIsOnline] = useState(false);
  const [shutdownDeadline, setShutdownDeadline] = useState<number | null>(null);
  const [countdown, setCountdown] = useState<string>('');
  
  const [forbiddenTime, setForbiddenTime] = useState<string | null>(null);
  const [cooldownDeadline, setCooldownDeadline] = useState<number | null>(null);
  const [cooldownCountdown, setCooldownCountdown] = useState<string>('');
  
  const [confirmWake, setConfirmWake] = useState(false);
  const [confirmShutdown, setConfirmShutdown] = useState(false);

  const handleMessage = useCallback((msg: any) => {
    if (msg.type === 'node_status') {
      setIsOnline(msg.payload.online_count > 0);
      setShutdownDeadline(msg.payload.shutdown_deadline || null);
      setForbiddenTime(msg.payload.forbidden_time || null);
      setCooldownDeadline(msg.payload.cooldown_deadline || null);
    } else if (msg.type === 'command_result') {
      if (msg.payload.success) {
        toast.success(msg.payload.message);
      } else {
        toast.error(msg.payload.message);
      }
    }
  }, []);

  const { isConnected, sendMessage } = useWebTransport(token, handleMessage);

  useEffect(() => {
    let interval: number;
    if (shutdownDeadline) {
      interval = window.setInterval(() => {
        const now = Math.floor(Date.now() / 1000);
        const remaining = shutdownDeadline - now;
        if (remaining <= 0) {
          setCountdown('即将关机');
        } else {
          const h = Math.floor(remaining / 3600);
          const m = Math.floor((remaining % 3600) / 60);
          const s = remaining % 60;
          setCountdown(`${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`);
        }
      }, 1000);
    } else {
      setCountdown('');
    }
    return () => window.clearInterval(interval);
  }, [shutdownDeadline]);

  useEffect(() => {
    let interval: number;
    if (cooldownDeadline && !isOnline) {
      interval = window.setInterval(() => {
        const now = Math.floor(Date.now() / 1000);
        const remaining = cooldownDeadline - now;
        if (remaining <= 0) {
          setCooldownCountdown('');
        } else {
          const h = Math.floor(remaining / 3600);
          const m = Math.floor((remaining % 3600) / 60);
          const s = remaining % 60;
          setCooldownCountdown(`${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`);
        }
      }, 1000);
    } else {
      setCooldownCountdown('');
    }
    return () => clearInterval(interval);
  }, [cooldownDeadline, isOnline]);

  const handleShutdown = () => {
    if (!confirmShutdown) {
      setConfirmShutdown(true);
      return;
    }
    sendMessage({ type: 'command', payload: { target_id: null, cmd: 'shutdown' } });
    setConfirmShutdown(false);
  };

  const handleWake = async () => {
    if (!confirmWake) {
      setConfirmWake(true);
      return;
    }
    
    try {
      const data = await fetchClient('/api/v1/system/start', { 
        method: 'POST'
      });
      if (data.success) {
        toast.success("唤醒指令已下发至米家终端！");
        setConfirmWake(false);
      } else {
        toast.error(`唤醒失败: ${data.message || '未知错误'}`);
        setConfirmWake(false);
      }
    } catch (e: any) {
      console.error(e);
    }
  };

  // 颜色联动逻辑
  // 如果 WebTransport 未连接，显示灰色离线状态
  const isWsOnline = isConnected;
  
  let bgLightColor = "bg-slate-300";
  let statusColor = "bg-slate-400";
  let statusText = "text-slate-500";
  let statusLabel = "服务端离线";

  if (isWsOnline) {
    bgLightColor = isOnline ? "bg-emerald-400" : "bg-rose-400";
    statusColor = isOnline ? "bg-emerald-500" : "bg-rose-500";
    statusText = isOnline ? "text-emerald-600" : "text-rose-600";
    statusLabel = isOnline ? "节点在线" : "节点休眠";
  }

  return (
    <div className="min-h-screen p-4 md:p-12 bg-slate-50 dark:bg-slate-900 relative overflow-hidden transition-colors duration-1000">
      <AmbientBackground color={bgLightColor} />

      <div className="max-w-7xl mx-auto relative z-10">
        <header className="flex flex-col md:flex-row justify-between items-start md:items-center gap-6 mb-10 md:mb-16">
          <h1 className="text-3xl md:text-4xl font-extrabold text-slate-900 dark:text-white tracking-tight transition-colors">
            Booter 控制台
          </h1>
          <div className="flex flex-wrap gap-3 md:gap-4 w-full md:w-auto">
            {role === 'admin' && (
              <button 
                onClick={() => window.location.href = '/admin'}
                className="text-sm bg-indigo-50 dark:bg-indigo-900/30 hover:bg-indigo-100 dark:hover:bg-indigo-900/50 px-6 py-2.5 rounded-full transition-colors border border-indigo-200 dark:border-indigo-800 text-indigo-700 dark:text-indigo-300 font-medium"
              >
                管理面板
              </button>
            )}
            <button 
              onClick={logout}
              className="text-sm bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700 px-6 py-2.5 rounded-full transition-colors border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-300 font-medium"
            >
              退出登录
            </button>
          </div>
        </header>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 md:gap-10">
          {/* Status Panel */}
          <GlassCard
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="p-6 md:p-10"
          >
            <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-8 transition-colors">系统状态</h2>
            <div className="space-y-6">
              <div className="flex justify-between items-center p-4 bg-slate-50 dark:bg-slate-900/50 border border-slate-100 dark:border-slate-800 rounded-xl transition-colors">
                <span className="text-slate-600 dark:text-slate-400 font-medium">宿主机节点</span>
                <span
                  className={`flex items-center gap-2 ${statusText} font-medium transition-colors duration-1000`}
                >
                  <span
                    className={`w-2.5 h-2.5 rounded-full ${statusColor} ${isWsOnline ? 'animate-[pulse_4s_ease-in-out_infinite]' : ''} transition-colors duration-1000`}
                  ></span>
                  {statusLabel}
                </span>
              </div>
              
              {countdown && (
                <div className="flex flex-col p-4 bg-slate-50 dark:bg-slate-900/50 border border-slate-100 dark:border-slate-800 rounded-xl transition-colors">
                  <div className="flex justify-between items-center w-full">
                    <span className="text-slate-600 dark:text-slate-400 font-medium">自动关机倒计时</span>
                    <span className="font-mono text-xl text-indigo-500 font-bold tracking-wider">
                      {countdown}
                    </span>
                  </div>
                  <span className="text-xs text-slate-400 dark:text-slate-500 mt-2">说明：倒计时可能根据服务活跃情况而自动重置</span>
                </div>
              )}

              {forbiddenTime && (
                <div className="flex justify-between items-center p-4 bg-slate-50 dark:bg-slate-900/50 border border-slate-100 dark:border-slate-800 rounded-xl transition-colors">
                  <span className="text-slate-600 dark:text-slate-400 font-medium">禁止开机时间段</span>
                  <div className="flex flex-col items-end">
                    <span className="font-mono text-md text-slate-700 dark:text-slate-300 font-bold tracking-wider">
                      {forbiddenTime}
                    </span>
                    <span className="text-xs text-slate-400 mt-1">({Intl.DateTimeFormat().resolvedOptions().timeZone})</span>
                  </div>
                </div>
              )}

              {cooldownCountdown && (
                <div className="flex justify-between items-center p-4 bg-slate-50 dark:bg-slate-900/50 border border-slate-100 dark:border-slate-800 rounded-xl transition-colors">
                  <span className="text-slate-600 dark:text-slate-400 font-medium">开机冷却倒计时</span>
                  <span className="font-mono text-xl text-orange-500 font-bold tracking-wider">
                    {cooldownCountdown}
                  </span>
                </div>
              )}
              
              <div className="flex justify-between items-center p-4 bg-slate-50 dark:bg-slate-900/50 border border-slate-100 dark:border-slate-800 rounded-xl transition-colors">
                <span className="text-slate-600 dark:text-slate-400 font-medium">WebTransport 连接状态</span>
                <span className={`text-sm font-semibold ${isConnected ? "text-emerald-500" : "text-rose-500"}`}>
                  {isConnected ? '已连接' : '已断开'}
                </span>
              </div>
            </div>
          </GlassCard>

          {/* Main Controls */}
          <GlassCard
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.1 }}
            className="md:col-span-2 p-6 md:p-8 flex flex-col min-h-[250px] md:min-h-[300px]"
          >
            <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-6 transition-colors">电源控制</h2>
            <div className="flex-1 flex w-full h-full">
              {!isOnline ? (
                <button
                  onClick={handleWake}
                  onMouseLeave={() => setConfirmWake(false)}
                  onBlur={() => setConfirmWake(false)}
                  className={`w-full h-full min-h-[200px] border rounded-2xl flex flex-col items-center justify-center transition-all group ${
                    confirmWake 
                      ? 'bg-amber-50 dark:bg-amber-950/30 hover:bg-amber-100 dark:hover:bg-amber-900/50 border-amber-200 dark:border-amber-800/50' 
                      : 'bg-emerald-50 dark:bg-emerald-950/30 hover:bg-emerald-100 dark:hover:bg-emerald-900/50 border-emerald-200 dark:border-emerald-800/50'
                  }`}
                >
                  <span className={`font-bold tracking-widest transition-transform ${
                    confirmWake 
                      ? 'text-amber-600 dark:text-amber-400 text-3xl' 
                      : 'text-emerald-600 dark:text-emerald-400 text-4xl group-hover:scale-110'
                  }`}>
                    {confirmWake ? "点击确认开机" : "唤醒电脑"}
                  </span>
                  {confirmWake && (
                    <span className="text-amber-600/70 dark:text-amber-400/70 mt-4 text-sm tracking-wide">
                      (点击空白处取消)
                    </span>
                  )}
                </button>
              ) : (
                <button
                  onClick={handleShutdown}
                  onMouseLeave={() => setConfirmShutdown(false)}
                  onBlur={() => setConfirmShutdown(false)}
                  className={`w-full h-full min-h-[200px] border rounded-2xl flex flex-col items-center justify-center transition-all group ${
                    confirmShutdown 
                      ? 'bg-rose-100 dark:bg-rose-900/50 border-rose-300 dark:border-rose-700/50' 
                      : 'bg-rose-50 dark:bg-rose-950/30 hover:bg-rose-100 dark:hover:bg-rose-900/50 border-rose-200 dark:border-rose-800/50'
                  }`}
                >
                  <span className={`font-bold tracking-widest transition-transform ${
                    confirmShutdown 
                      ? 'text-rose-700 dark:text-rose-300 text-3xl' 
                      : 'text-rose-600 dark:text-rose-400 text-4xl group-hover:scale-110'
                  }`}>
                    {confirmShutdown ? "点击确认关机" : "关机"}
                  </span>
                  {confirmShutdown && (
                    <span className="text-rose-700/70 dark:text-rose-300/70 mt-4 text-sm tracking-wide">
                      (点击空白处取消)
                    </span>
                  )}
                </button>
              )}
            </div>
          </GlassCard>
        </div>

        {role === 'admin' && (
          <>
            <AdminDelayShutdown />
            <LogsPanel />
          </>
        )}
      </div>
    </div>
  );
}
