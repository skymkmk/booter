

interface AmbientBackgroundProps {
  color?: string;
}

export function AmbientBackground({
  color = "bg-indigo-400",
}: AmbientBackgroundProps) {
  return (
    <>
      <style>
        {`
          @keyframes ambient-fade-in {
            0% { opacity: 0; }
            100% { opacity: 0.25; }
          }
          @keyframes deep-pulse {
            0%, 100% { opacity: 0.25; transform: scale(1); }
            50% { opacity: 0.5; transform: scale(1.1); }
          }
        `}
      </style>
      <div className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[80vmax] h-[80vmax] max-w-[1000px] max-h-[1000px] pointer-events-none z-0">
        <div className="w-full h-full relative animate-[spin_120s_linear_infinite]">
          <div
            className={`absolute top-0 left-0 w-[60%] h-[60%] rounded-full filter blur-3xl opacity-0 transition-colors duration-1000 ${color}`}
            style={{
              animation:
                "ambient-fade-in 5s ease-out forwards, deep-pulse 20s cubic-bezier(0.4, 0, 0.6, 1) 5s infinite",
            }}
          ></div>
          <div
            className={`absolute bottom-0 right-0 w-[60%] h-[60%] rounded-full filter blur-3xl hue-rotate-[30deg] opacity-0 transition-colors duration-1000 ${color}`}
            style={{
              animation:
                "ambient-fade-in 5s ease-out forwards, deep-pulse 20s cubic-bezier(0.4, 0, 0.6, 1) 15s infinite",
            }}
          ></div>
        </div>
      </div>
    </>
  );
}
