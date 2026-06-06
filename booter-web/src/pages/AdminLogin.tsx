import React, { useState } from "react";
import { motion } from "framer-motion";
import { useNavigate } from "react-router-dom";
import { AmbientBackground } from "../components/AmbientBackground";
import { GlassCard } from "../components/GlassCard";
import { useAuth } from "../context/AuthContext";
import { Turnstile } from "@marsidev/react-turnstile";
import { fetchClient } from "../utils/fetchClient";

export default function AdminLogin() {
  const navigate = useNavigate();
  const { login } = useAuth();
  const [totp, setTotp] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [siteKey, setSiteKey] = useState<string | null>(null);
  const [turnstileToken, setTurnstileToken] = useState<string | null>(null);

  React.useEffect(() => {
    // Route pre-check
    const token = localStorage.getItem("booter_token");
    if (token) {
      fetchClient("/api/v1/auth/me")
        .then(() => {
          navigate("/dashboard");
        })
        .catch(() => {
          // Token is invalid, fetchClient will clear it automatically
        });
    }

    fetchClient("/api/v1/system/turnstile")
      .then((data) => {
        if (data.site_key) {
          setSiteKey(data.site_key);
        }
      })
      .catch((err) => console.error("Failed to load Turnstile config", err));
  }, [navigate]);

  const handleVerifyTotp = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!totp || totp.length !== 6) return;
    
    setLoading(true);
    setError(null);

    // [MOCK 幻境] 用于前端脱机体验
    if (totp === "654321") {
      setTimeout(() => {
        login("mock_admin_token", "admin");
        navigate("/admin");
        setLoading(false);
      }, 800);
      return;
    }

    try {
      const data = await fetchClient("/api/v1/auth/admin/totp", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code: totp, turnstile_token: turnstileToken }),
      });
      
      if (data.success && data.token) {
        login(data.token, data.role || "admin");
        navigate("/admin");
      } else {
        setError(data.message || "动态密码错误或失效");
      }
    } catch (err) {
      setError("网络错误，无法连接到后端服务器（可使用 654321 体验 Mock）");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="dark">
      <div className="min-h-screen flex items-center justify-center p-6 relative overflow-hidden bg-slate-50 dark:bg-slate-900 transition-colors duration-500">
        <AmbientBackground color="bg-rose-500" />

        <GlassCard
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.5, ease: "easeOut" }}
          className="w-full max-w-md p-10 z-10 border-rose-500/20 shadow-[0_0_40px_-10px_rgba(244,63,94,0.15)] dark:shadow-[0_0_40px_-10px_rgba(244,63,94,0.3)]"
        >
          <div className="text-center mb-10">
            <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-rose-500/10 mb-4">
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6 text-rose-600 dark:text-rose-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
              </svg>
            </div>
            <h1 className="text-3xl font-bold tracking-tight text-slate-900 dark:text-white mb-2 transition-colors">
              超级终端授权
            </h1>
            <p className="text-rose-600/70 dark:text-rose-200/60 text-sm transition-colors">此区域仅限系统所有者访问</p>
          </div>

          <motion.form
            onSubmit={handleVerifyTotp}
            className="space-y-6"
          >
            <div>
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2 transition-colors">
                Authenticator 动态口令
              </label>
              <input
                type="text"
                required
                maxLength={6}
                value={totp}
                onChange={(e) => setTotp(e.target.value.replace(/\D/g, ''))}
                className="w-full bg-white/50 dark:bg-slate-950/50 border border-slate-200 dark:border-slate-700/50 rounded-xl px-4 py-4 text-slate-900 dark:text-white placeholder-slate-400 dark:placeholder-slate-600 focus:outline-none focus:ring-2 focus:ring-rose-500/50 focus:border-rose-500/50 transition-all text-center tracking-[0.5em] text-2xl font-mono font-medium"
                placeholder="------"
              />
            </div>

            {error && (
              <p className="text-rose-500 dark:text-rose-400 text-sm font-medium text-center">{error}</p>
            )}

            {siteKey && (
              <div className="flex justify-center my-4 [&>iframe]:rounded-xl overflow-hidden">
                <Turnstile 
                  siteKey={siteKey} 
                  onSuccess={setTurnstileToken} 
                  options={{ theme: "dark" }}
                />
              </div>
            )}

            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              disabled={loading || totp.length !== 6 || (siteKey !== null && !turnstileToken)}
              type="submit"
              className="w-full bg-rose-600 hover:bg-rose-500 disabled:bg-rose-300 dark:disabled:bg-rose-900/50 disabled:text-white/70 dark:disabled:text-rose-200/30 text-white font-medium py-4 rounded-xl transition-colors"
            >
              {loading ? "验证中..." : "授权访问"}
            </motion.button>
          </motion.form>
        </GlassCard>
      </div>
    </div>
  );
}
