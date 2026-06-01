import React, { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useNavigate } from "react-router-dom";
import { AmbientBackground } from "../components/AmbientBackground";
import { GlassCard } from "../components/GlassCard";
import { useAuth } from "../context/AuthContext";
import { Turnstile } from "@marsidev/react-turnstile";
import { fetchClient } from "../utils/fetchClient";

export default function Login() {
  const navigate = useNavigate();
  const { login } = useAuth();
  const [email, setEmail] = useState("");
  const [otp, setOtp] = useState("");
  const [step, setStep] = useState<0 | 1>(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [siteKey, setSiteKey] = useState<string | null>(null);
  const [turnstileToken, setTurnstileToken] = useState<string | null>(null);

  React.useEffect(() => {
    fetchClient("/api/v1/system/turnstile")
      .then((data) => {
        if (data.site_key) {
          setSiteKey(data.site_key);
        }
      })
      .catch((err) => console.error("Failed to load Turnstile config", err));
  }, []);

  const handleRequestOtp = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email) return;
    
    setLoading(true);
    setError(null);

    try {
      const data = await fetchClient("/api/v1/auth/email/request", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, turnstile_token: turnstileToken }),
      });
      
      if (data.success) {
        setStep(1);
      } else {
        setError(data.message || "Failed to send OTP");
      }
    } catch (err) {
      setError("网络错误，无法连接到后端服务器");
    } finally {
      setLoading(false);
    }
  };

  const handleVerifyOtp = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!otp) return;
    
    setLoading(true);
    setError(null);

    try {
      const data = await fetchClient("/api/v1/auth/email/verify", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, code: otp }),
      });
      
      if (data.success && data.token) {
        login(data.token, data.role || "user");
        navigate("/dashboard");
      } else {
        setError(data.message || "验证码错误");
      }
    } catch (err) {
      setError("网络错误，无法连接到后端服务器");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center p-6 relative overflow-hidden bg-slate-50">
      <AmbientBackground color="bg-indigo-400" />

      <GlassCard
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: "easeOut" }}
        className="w-full max-w-md p-10 z-10"
      >
        <div className="text-center mb-10">
          <h1 className="text-4xl font-bold tracking-tight text-slate-900 mb-2">
            Booter
          </h1>
          <p className="text-slate-500 text-sm">安全远程控制平面</p>
        </div>

        <div className="space-y-6">
          <AnimatePresence mode="wait">
            {step === 0 ? (
              <motion.form
                key="step0"
                initial={{ opacity: 0, x: -20 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 20 }}
                transition={{ duration: 0.3 }}
                onSubmit={handleRequestOtp}
                className="space-y-6"
              >
                <div>
                  <label className="block text-sm font-medium text-slate-700 mb-2">
                    邮箱地址
                  </label>
                  <input
                    type="email"
                    required
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    className="w-full bg-white/50 border border-slate-200 rounded-xl px-4 py-3 text-slate-900 placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
                    placeholder="admin@example.com"
                  />
                </div>

                {error && (
                  <p className="text-rose-500 text-sm font-medium">{error}</p>
                )}

                {siteKey && (
                  <div className="flex justify-center my-4">
                    <Turnstile 
                      siteKey={siteKey} 
                      onSuccess={setTurnstileToken} 
                      options={{ theme: "light" }}
                    />
                  </div>
                )}

                <motion.button
                  whileHover={{ scale: 1.02 }}
                  whileTap={{ scale: 0.98 }}
                  disabled={loading || (siteKey !== null && !turnstileToken)}
                  type="submit"
                  className="w-full bg-indigo-600 hover:bg-indigo-700 disabled:bg-indigo-400 text-white font-medium py-3 rounded-xl transition-colors"
                >
                  {loading ? "发送中..." : "发送验证码"}
                </motion.button>
              </motion.form>
            ) : (
              <motion.form
                key="step1"
                initial={{ opacity: 0, x: -20 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 20 }}
                transition={{ duration: 0.3 }}
                onSubmit={handleVerifyOtp}
                className="space-y-6"
              >
                <div>
                  <label className="block text-sm font-medium text-slate-700 mb-2 flex justify-between items-center">
                    <span>验证码</span>
                    <button 
                      type="button" 
                      onClick={() => {
                        setStep(0);
                        setError(null);
                      }}
                      className="text-xs text-indigo-500 hover:text-indigo-700 transition-colors"
                    >
                      修改邮箱
                    </button>
                  </label>
                  <input
                    type="text"
                    required
                    maxLength={6}
                    value={otp}
                    onChange={(e) => setOtp(e.target.value)}
                    className="w-full bg-white/50 border border-slate-200 rounded-xl px-4 py-3 text-slate-900 placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all text-center tracking-[0.5em] text-lg font-mono font-medium"
                    placeholder="------"
                  />
                </div>

                {error && (
                  <p className="text-rose-500 text-sm font-medium">{error}</p>
                )}

                <motion.button
                  whileHover={{ scale: 1.02 }}
                  whileTap={{ scale: 0.98 }}
                  disabled={loading}
                  type="submit"
                  className="w-full bg-indigo-600 hover:bg-indigo-700 disabled:bg-indigo-400 text-white font-medium py-3 rounded-xl transition-colors"
                >
                  {loading ? "验证中..." : "登录控制台"}
                </motion.button>
              </motion.form>
            )}
          </AnimatePresence>
        </div>
      </GlassCard>
    </div>
  );
}
