import { useState, useEffect } from 'react';
import { GlassCard } from './GlassCard';
import Editor from '@monaco-editor/react';
import { fetchClient } from '../utils/fetchClient';

interface Companion {
  id: string;
  name: string;
  scripts: string;
  created_at: string;
}

export function CompanionsPanel({ token }: { token: string | null }) {
  const [companions, setCompanions] = useState<Companion[]>([]);
  const [loading, setLoading] = useState(true);
  
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editProbes, setEditProbes] = useState<{name: string, code: string}[]>([]);
  
  const [saving, setSaving] = useState(false);

  const fetchCompanions = async () => {
    try {
      const data = await fetchClient('/api/v1/admin/companions', {
        headers: { 'Authorization': `Bearer ${token}` }
      });
      if (Array.isArray(data)) {
        setCompanions(data);
      }
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (token) fetchCompanions();
  }, [token]);

  const handleAdd = async () => {
    try {
      await fetchClient('/api/v1/admin/companions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`
        },
        body: JSON.stringify({ name: '新伴侣主机', scripts: {} })
      });
      fetchCompanions();
    } catch (err) {
      console.error(err);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm('确定要删除这个伴侣配置吗？该伴侣将无法连接。')) return;
    try {
      await fetchClient(`/api/v1/admin/companions/${id}`, {
        method: 'DELETE',
        headers: { 'Authorization': `Bearer ${token}` }
      });
      fetchCompanions();
    } catch (err) {
      console.error(err);
    }
  };

  const handleSaveEdit = async () => {
    if (!editingId) return;
    setSaving(true);
    try {
      const parsed: Record<string, string> = {};
      for (const p of editProbes) {
        if (p.name.trim()) {
          parsed[p.name.trim()] = p.code;
        }
      }

      await fetchClient(`/api/v1/admin/companions/${editingId}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`
        },
        body: JSON.stringify({ name: editName, scripts: parsed })
      });
      setEditingId(null);
      fetchCompanions();
    } catch (err) {
      console.error(err);
    } finally {
      setSaving(false);
    }
  };

  const startEdit = (c: Companion) => {
    setEditingId(c.id);
    setEditName(c.name);
    try {
      const parsed = JSON.parse(c.scripts);
      const probes = Object.entries(parsed).map(([name, code]) => ({ name, code: code as string }));
      setEditProbes(probes);
    } catch (e) {
      setEditProbes([]);
    }
  };

  const addProbe = () => {
    setEditProbes([...editProbes, { name: '', code: '' }]);
  };

  const removeProbe = (index: number) => {
    setEditProbes(editProbes.filter((_, i) => i !== index));
  };

  const updateProbe = (index: number, field: 'name' | 'code', value: string) => {
    const newProbes = [...editProbes];
    newProbes[index][field] = value;
    setEditProbes(newProbes);
  };

  return (
    <GlassCard className="p-10 mb-12">
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-2xl font-bold text-slate-900 dark:text-white transition-colors">伴侣管理中心</h2>
        <button
          onClick={handleAdd}
          className="bg-emerald-600 hover:bg-emerald-700 text-white font-medium px-6 py-2 rounded-xl transition-colors text-sm"
        >
          + 新增伴侣
        </button>
      </div>
      <p className="text-slate-500 dark:text-slate-400 mb-8 transition-colors">
        在此处配置所有受控端（伴侣）。新增伴侣后，使用生成的 UUID 作为 <code className="bg-slate-200 dark:bg-slate-800 px-1 rounded">BOOTER_CLIENT_ID</code> 启动伴侣程序。
      </p>

      {editingId ? (
        <div className="border border-slate-200 dark:border-slate-700 rounded-xl p-6 bg-slate-50 dark:bg-[#1a1a1a]">
          <h3 className="text-lg font-bold mb-4 dark:text-white">编辑伴侣配置</h3>
          
          <label className="block mb-2 text-sm font-medium text-slate-700 dark:text-slate-300">伴侣名称</label>
          <input
            type="text"
            value={editName}
            onChange={e => setEditName(e.target.value)}
            className="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg px-4 py-2 text-slate-900 dark:text-white mb-6 focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />

          <div className="flex justify-between items-center mb-4">
            <label className="text-sm font-medium text-slate-700 dark:text-slate-300">专属探针脚本配置 (Rhai 语法)</label>
            <button
              onClick={addProbe}
              className="bg-emerald-600 hover:bg-emerald-700 text-white font-medium px-4 py-1.5 rounded-lg transition-colors text-xs"
            >
              + 添加新探针
            </button>
          </div>
          
          <div className="space-y-6 mb-6">
            {editProbes.length === 0 ? (
              <div className="text-center text-slate-500 py-8 bg-white/50 dark:bg-slate-900/50 rounded-lg border border-dashed border-slate-300 dark:border-slate-700">
                当前没有配置任何探针，伴侣程序只会执行系统指令和基础连接心跳。
              </div>
            ) : (
              editProbes.map((probe, index) => (
                <div key={index} className="bg-white dark:bg-[#1e1e1e] border border-slate-200 dark:border-slate-700 rounded-xl overflow-hidden shadow-sm">
                  <div className="flex items-center justify-between px-4 py-3 border-b border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800/50">
                    <input
                      type="text"
                      placeholder="探针名称 (例如: 探测 Jellyfin)"
                      value={probe.name}
                      onChange={(e) => updateProbe(index, 'name', e.target.value)}
                      className="bg-transparent border-none focus:ring-0 text-sm font-semibold text-slate-900 dark:text-white w-1/2 placeholder-slate-400 px-2 py-1 outline-none"
                    />
                    <button
                      onClick={() => removeProbe(index)}
                      className="text-rose-500 hover:text-rose-600 dark:hover:text-rose-400 text-sm font-medium transition-colors px-2 py-1"
                    >
                      删除探针
                    </button>
                  </div>
                  <div className="h-48 relative pt-2">
                    <Editor
                      height="100%"
                      defaultLanguage="rust"
                      theme="vs-dark"
                      value={probe.code}
                      onChange={(val) => updateProbe(index, 'code', val || '')}
                      options={{ minimap: { enabled: false }, fontSize: 13, scrollBeyondLastLine: false }}
                    />
                  </div>
                </div>
              ))
            )}
          </div>

          <div className="flex gap-4">
            <button
              onClick={handleSaveEdit}
              disabled={saving}
              className="bg-indigo-600 hover:bg-indigo-700 text-white font-medium px-6 py-2 rounded-xl transition-colors"
            >
              {saving ? '保存中...' : '保存修改'}
            </button>
            <button
              onClick={() => setEditingId(null)}
              className="bg-slate-200 hover:bg-slate-300 dark:bg-slate-700 dark:hover:bg-slate-600 text-slate-800 dark:text-white font-medium px-6 py-2 rounded-xl transition-colors"
            >
              取消
            </button>
          </div>
        </div>
      ) : (
        <div className="border border-slate-200 dark:border-slate-700 rounded-xl overflow-hidden overflow-x-auto">
          {loading ? (
            <div className="p-4 text-center text-slate-500">加载中...</div>
          ) : companions.length === 0 ? (
            <div className="p-8 text-center text-slate-500">当前没有配置任何伴侣</div>
          ) : (
            <table className="w-full text-left text-sm text-slate-600 dark:text-slate-300">
              <thead className="bg-slate-50 dark:bg-slate-800/50 text-xs uppercase font-semibold text-slate-500 dark:text-slate-400">
                <tr>
                  <th className="px-6 py-4 border-b border-slate-200 dark:border-slate-700">伴侣名称</th>
                  <th className="px-6 py-4 border-b border-slate-200 dark:border-slate-700">UUID (Client ID)</th>
                  <th className="px-6 py-4 border-b border-slate-200 dark:border-slate-700">探针数量</th>
                  <th className="px-6 py-4 border-b border-slate-200 dark:border-slate-700 text-right">操作</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-200 dark:divide-slate-700 bg-white dark:bg-[#1e1e1e]">
                {companions.map(c => {
                  let probesCount = 0;
                  try {
                    probesCount = Object.keys(JSON.parse(c.scripts)).length;
                  } catch (e) {}

                  return (
                    <tr key={c.id} className="hover:bg-slate-50 dark:hover:bg-slate-800/30 transition-colors">
                      <td className="px-6 py-4 font-medium text-slate-900 dark:text-white">
                        {c.name}
                      </td>
                      <td className="px-6 py-4 font-mono text-xs text-indigo-500 dark:text-indigo-400">
                        {c.id}
                      </td>
                      <td className="px-6 py-4">
                        <span className="bg-indigo-50 dark:bg-indigo-900/30 text-indigo-600 dark:text-indigo-400 py-1 px-3 rounded-full text-xs font-medium">
                          {probesCount} 个脚本
                        </span>
                      </td>
                      <td className="px-6 py-4 text-right space-x-3">
                        <button
                          onClick={() => startEdit(c)}
                          className="font-medium text-indigo-600 dark:text-indigo-400 hover:underline"
                        >
                          编辑
                        </button>
                        <button
                          onClick={() => handleDelete(c.id)}
                          className="font-medium text-rose-500 hover:underline"
                        >
                          删除
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      )}
    </GlassCard>
  );
}
