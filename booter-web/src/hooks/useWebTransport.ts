import { useState, useEffect, useRef, useCallback } from "react";

export type DashboardToServer =
  | { type: "command"; payload: { target_id: string | null; cmd: string } };

export type ServerToDashboard =
  | {
      type: "status";
      payload: {
        client_id: string;
        active: boolean;
        active_service: string | null;
        probe_type: string;
      };
    }
  | { type: "command_result"; payload: { success: boolean; message: string } }
  | {
      type: "node_status";
      payload: {
        online_count: number;
        shutdown_deadline: number | null;
        forbidden_time: string | null;
        cooldown_deadline: number | null;
        absolute_cooldown_deadline: number | null;
      };
    };

export function useWebTransport(token: string | null, onMessage?: (msg: ServerToDashboard) => void) {
  const [isConnected, setIsConnected] = useState(false);
  const [lastMessage, setLastMessage] = useState<ServerToDashboard | null>(null);

  const wtRef = useRef<any>(null);
  const writerRef = useRef<WritableStreamDefaultWriter<Uint8Array> | null>(
    null,
  );
  const reconnectTimeoutRef = useRef<number | null>(null);

  const connect = useCallback(async () => {
    try {
      // 1. Initialize WebTransport on UDP 8080
      const wtUrl = `https://${window.location.hostname}:8080/api/v1/system/wt`;
      console.log(`[WebTransport] Connecting to ${wtUrl}`);
      
      const wt = new WebTransport(wtUrl, {
        allowPooling: false,
      } as any);

      wtRef.current = wt;

      // Wait for connection to be ready
      await wt.ready;
      console.log(`[WebTransport] Connected successfully!`);
      setIsConnected(true);

      // 3. Create Bidirectional stream
      const stream = await wt.createBidirectionalStream();
      const writer = stream.writable.getWriter();
      writerRef.current = writer;

      // Authenticate
      if (token) {
        const encoder = new TextEncoder();
        writer.write(encoder.encode(JSON.stringify({ type: "auth", payload: { token: token } }) + "\n"));
      }

      // 4. Read loop
      const reader = stream.readable.getReader();
      const decoder = new TextDecoder();
      let leftover = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) {
          console.log("[WebTransport] Stream closed");
          break;
        }

        leftover += decoder.decode(value, { stream: true });
        let newlineIdx;
        while ((newlineIdx = leftover.indexOf("\n")) !== -1) {
          const line = leftover.substring(0, newlineIdx).trim();
          leftover = leftover.substring(newlineIdx + 1);

          if (line) {
            try {
              const parsed = JSON.parse(line) as ServerToDashboard;
                if (parsed.type === "command_result" && !parsed.payload.success && parsed.payload.message === "Authentication failed") {
                  localStorage.removeItem('booter_token');
                  localStorage.removeItem('booter_role');
                  window.location.href = '/login';
                  return;
                }
                
                setLastMessage(parsed);
              if (onMessage) {
                onMessage(parsed);
              }
            } catch (err) {
              console.error("[WebTransport] Failed to parse message:", line);
            }
          }
        }
      }

      setIsConnected(false);
      reconnectTimeoutRef.current = window.setTimeout(() => {
        connect();
      }, 5000);
    } catch (err) {
      console.error("[WebTransport] Error/Disconnected:", err);
      setIsConnected(false);

      // Auto reconnect
      reconnectTimeoutRef.current = window.setTimeout(() => {
        connect();
      }, 5000);
    }
  }, [token, onMessage]);

  useEffect(() => {
    connect();

    return () => {
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
      if (wtRef.current) {
        try {
          wtRef.current.close();
        } catch (e) {}
      }
    };
  }, [connect, token]);

  const sendMessage = useCallback(
    (msg: DashboardToServer) => {
      if (writerRef.current && isConnected) {
        try {
          const encoder = new TextEncoder();
          writerRef.current.write(encoder.encode(JSON.stringify(msg) + "\n"));
        } catch (err) {
          console.error("[WebTransport] Send error:", err);
        }
      } else {
        console.warn("[WebTransport] Cannot send message, not connected.");
      }
    },
    [isConnected],
  );

  return { isConnected, lastMessage, sendMessage };
}
