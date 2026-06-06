import { useState } from 'react';
import { fetchClient } from '../utils/fetchClient';
import { GlassCard } from './GlassCard';
import { toast } from 'sonner';

export function AdminDelayShutdown() {
  const [minutes, setMinutes] = useState<number>(30);
  const [loading, setLoading] = useState(false);

  const handleDelay = async () => {
    setLoading(true);
    try {
      const data = await fetchClient('/api/v1/admin/delay', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ minutes })
      });
      if (data.success) {
        toast.success(`延迟关机时间已调整 ${minutes} 分钟`);
      } else {
        toast.error(`调整失败: ${data.message}`);
      }
    } catch (e: any) {
      // Error handled by fetchClient
    } finally {
      setLoading(false);
    }
  };

  return (
    <GlassCard className="p-6 mt-6">
      <h2 className="text-xl font-bold text-slate-900 dark:text-white mb-4">延迟关机设置 (管理员)</h2>
      <div className="flex flex-col md:flex-row gap-4 items-center">
        <div className="flex-1 w-full">
          <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-1">
            调整时间 (分钟，正数为增加，负数为减少)
          </label>
          <input 
            type="number" 
            value={minutes}
            onChange={(e) => setMinutes(parseInt(e.target.value) || 0)}
            className="w-full px-4 py-2 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 text-slate-900 dark:text-white transition-colors"
          />
        </div>
        <button
          onClick={handleDelay}
          disabled={loading}
          className="w-full md:w-auto px-6 py-2.5 bg-indigo-600 hover:bg-indigo-700 text-white font-medium rounded-lg transition-colors shadow-sm disabled:opacity-50 mt-1 md:mt-5"
        >
          {loading ? '提交中...' : '调整延迟'}
        </button>
      </div>
    </GlassCard>
  );
}
