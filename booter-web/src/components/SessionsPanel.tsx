import { useState, useEffect } from 'react';
import { fetchClient } from '../utils/fetchClient';
import { GlassCard } from './GlassCard';
import { toast } from 'sonner';

interface SessionItem {
  token: string;
  role: string;
  email: string;
  created_at: string;
  last_used_at: string;
}

export function SessionsPanel() {
  const [sessions, setSessions] = useState<SessionItem[]>([]);
  const [loading, setLoading] = useState(true);

  const loadSessions = async () => {
    setLoading(true);
    try {
      const data = await fetchClient('/api/v1/admin/sessions');
      if (data.success && data.sessions) {
        setSessions(data.sessions);
      }
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSessions();
  }, []);

  const handleRevoke = async (tokenStr: string) => {
    if (!window.confirm("确定要强制踢出该会话吗？该用户将立即掉线。")) return;
    try {
      const data = await fetchClient(`/api/v1/admin/sessions/${tokenStr}`, {
        method: 'DELETE'
      });
      if (data.success) {
        toast.success("会话已踢出");
        loadSessions();
      } else {
        toast.error(`踢出失败: ${data.message}`);
      }
    } catch (e) {
      // fetchClient handles toast
    }
  };

  return (
    <GlassCard className="p-6 md:p-8 mt-10">
      <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-6">活跃会话管理</h2>
      <div className="overflow-x-auto">
        <table className="w-full text-left text-sm text-slate-600 dark:text-slate-300">
          <thead className="bg-slate-100 dark:bg-slate-800/50 text-slate-700 dark:text-slate-300 uppercase font-semibold">
            <tr>
              <th className="px-4 py-3 rounded-tl-lg">用户</th>
              <th className="px-4 py-3">角色</th>
              <th className="px-4 py-3">最后活跃时间</th>
              <th className="px-4 py-3 rounded-tr-lg">操作</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100 dark:divide-slate-800/50">
            {sessions.map(session => (
              <tr key={session.token} className="hover:bg-slate-50 dark:hover:bg-slate-800/30 transition-colors">
                <td className="px-4 py-3">{session.email}</td>
                <td className="px-4 py-3">
                  <span className={`px-2 py-1 rounded text-xs font-medium ${
                    session.role === 'admin' ? 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400' : 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-400'
                  }`}>
                    {session.role}
                  </span>
                </td>
                <td className="px-4 py-3 font-mono text-xs">{session.last_used_at}</td>
                <td className="px-4 py-3">
                  <button 
                    onClick={() => handleRevoke(session.token)}
                    className="text-xs bg-rose-50 text-rose-600 dark:bg-rose-900/30 dark:text-rose-400 hover:bg-rose-100 dark:hover:bg-rose-900/50 px-3 py-1.5 rounded transition-colors"
                  >
                    强制踢出
                  </button>
                </td>
              </tr>
            ))}
            {sessions.length === 0 && !loading && (
              <tr>
                <td colSpan={4} className="px-4 py-8 text-center text-slate-500">
                  暂无活跃会话
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </GlassCard>
  );
}
