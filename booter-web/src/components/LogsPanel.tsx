import { useState, useEffect } from 'react';
import { fetchClient } from '../utils/fetchClient';
import { GlassCard } from './GlassCard';
import { motion } from 'framer-motion';

interface LogItem {
  id: number;
  email: string;
  action: string;
  created_at: string;
}

export function LogsPanel() {
  const [logs, setLogs] = useState<LogItem[]>([]);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [hasMore, setHasMore] = useState(true);

  const fetchLogs = async (p: number) => {
    setLoading(true);
    try {
      const data = await fetchClient(`/api/v1/admin/logs?page=${p}`);
      if (data.success) {
        if (p === 1) {
          setLogs(data.logs);
        } else {
          setLogs(prev => [...prev, ...data.logs]);
        }
        if (data.logs.length < 10) {
          setHasMore(false);
        } else {
          setHasMore(true);
        }
      }
    } catch (e) {
      console.error("Failed to load logs", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchLogs(1);
  }, []);

  const loadMore = () => {
    const nextPage = page + 1;
    setPage(nextPage);
    fetchLogs(nextPage);
  };

  return (
    <GlassCard className="p-6 md:p-8 mt-6">
      <h2 className="text-2xl font-bold text-slate-900 dark:text-white mb-6">用户操作日志</h2>
      <div className="overflow-x-auto">
        <table className="w-full text-left text-sm text-slate-600 dark:text-slate-300">
          <thead className="bg-slate-100 dark:bg-slate-800/50 text-slate-700 dark:text-slate-300 uppercase font-semibold">
            <tr>
              <th className="px-4 py-3 rounded-tl-lg">时间</th>
              <th className="px-4 py-3">用户</th>
              <th className="px-4 py-3 rounded-tr-lg">操作</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100 dark:divide-slate-800/50">
            {logs.map(log => (
              <motion.tr 
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                key={log.id} 
                className="hover:bg-slate-50 dark:hover:bg-slate-800/30 transition-colors"
              >
                <td className="px-4 py-3 font-mono text-xs">{log.created_at}</td>
                <td className="px-4 py-3">{log.email}</td>
                <td className="px-4 py-3">
                  <span className={`px-2 py-1 rounded text-xs font-medium ${
                    log.action === 'Wakeup' ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400' :
                    log.action === 'Shutdown' ? 'bg-rose-100 text-rose-700 dark:bg-rose-900/30 dark:text-rose-400' :
                    'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-400'
                  }`}>
                    {log.action}
                  </span>
                </td>
              </motion.tr>
            ))}
            {logs.length === 0 && !loading && (
              <tr>
                <td colSpan={3} className="px-4 py-8 text-center text-slate-500">
                  暂无日志记录
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {hasMore && logs.length > 0 && (
        <div className="mt-4 flex justify-center">
          <button 
            onClick={loadMore}
            disabled={loading}
            className="text-sm bg-indigo-50 dark:bg-indigo-900/30 hover:bg-indigo-100 dark:hover:bg-indigo-900/50 px-6 py-2 rounded-full transition-colors border border-indigo-200 dark:border-indigo-800 text-indigo-700 dark:text-indigo-300 font-medium disabled:opacity-50"
          >
            {loading ? '加载中...' : '加载更多'}
          </button>
        </div>
      )}
    </GlassCard>
  );
}
