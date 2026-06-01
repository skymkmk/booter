import React from "react";
import { motion } from "framer-motion";

export type GlassCardProps = React.ComponentProps<typeof motion.div>;

export const GlassCard = React.forwardRef<HTMLDivElement, GlassCardProps>(
  ({ className = "", children, ...props }, ref) => {
    return (
      <motion.div
        ref={ref}
        className={`rounded-3xl bg-white/70 dark:bg-neutral-800/70 backdrop-saturate-200 backdrop-blur-md backdrop-brightness-110 ring-1 ring-neutral-300/70 dark:ring-white/20 ${className}`}
        {...props}
      >
        {children}
      </motion.div>
    );
  },
);

GlassCard.displayName = "GlassCard";
