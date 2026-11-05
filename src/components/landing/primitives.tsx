import { Star } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

export function Stars({ className, size = 14 }: { className?: string; size?: number }) {
  return (
    <span className={cn("inline-flex gap-0.5 text-ink", className)} aria-label="5 out of 5 stars">
      {Array.from({ length: 5 }).map((_, i) => (
        <Star key={i} width={size} height={size} className="fill-current" strokeWidth={0} />
      ))}
    </span>
  );
}

export function Eyebrow({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <p className={cn("text-[11px] font-bold uppercase tracking-[0.14em] text-ink/70", className)}>
      {children}
    </p>
  );
}

export function Heading({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <h2
      className={cn(
        "font-display text-[30px] font-bold leading-[1.05] tracking-[-0.02em] text-ink",
        className,
      )}
    >
      {children}
    </h2>
  );
}

export function Section({
  id,
  children,
  className,
}: {
  id?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section id={id} className={cn("scroll-mt-16 px-5 py-10", className)}>
      {children}
    </section>
  );
}

export function CtaButton({
  children = "Try it now",
  variant = "brand",
  className,
  href = "#buy",
}: {
  children?: ReactNode;
  variant?: "brand" | "ink";
  className?: string;
  href?: string;
}) {
  return (
    <a
      href={href}
      className={cn(
        "flex h-14 w-full items-center justify-center rounded-2xl font-display text-lg font-bold tracking-tight transition active:scale-[0.98]",
        variant === "brand"
          ? "bg-brand text-white shadow-[0_8px_24px_-8px] shadow-brand/60 hover:bg-brand-deep"
          : "bg-ink text-white hover:bg-ink/90",
        className,
      )}
    >
      {children}
    </a>
  );
}

/** Stylised holographic pouch, used until real product photography is dropped in. */
export function Pouch({ className, small }: { className?: string; small?: boolean }) {
  return (
    <div
      className={cn(
        "holo relative flex flex-col items-center overflow-hidden rounded-[18px] shadow-[0_18px_40px_-16px_rgba(60,20,80,0.45)] ring-1 ring-white/60",
        small ? "w-24 px-2 pb-3 pt-4" : "w-52 px-4 pb-6 pt-8",
        className,
      )}
    >
      <div className="absolute inset-x-0 top-0 h-3 bg-white/40" />
      <span
        className={cn(
          "font-display font-extrabold leading-[0.85] tracking-[-0.04em] text-brand",
          small ? "text-[26px]" : "text-[58px]",
        )}
      >
        hello
        <br />
        nutra
      </span>
      <span
        className={cn(
          "mt-2 text-center font-bold leading-tight text-brand",
          small ? "text-[6px]" : "text-[11px]",
        )}
      >
        Vaginal Probiotic
        <br />
        Gummies
      </span>
      {!small && (
        <div className="mt-5 flex gap-2">
          {["pH", "odor", "flora"].map((t) => (
            <span
              key={t}
              className="grid size-9 place-items-center rounded-full border border-brand/50 text-[9px] font-bold text-brand"
            >
              {t}
            </span>
          ))}
        </div>
      )}
      <div
        className={cn("mt-auto w-full rounded-sm bg-sun/80", small ? "mt-2 h-1.5" : "mt-6 h-3")}
      />
    </div>
  );
}

export function Gummy({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        "block rounded-[45%_45%_40%_40%/55%_55%_45%_45%] bg-[radial-gradient(circle_at_35%_30%,#ffe890,#ffc21a_55%,#e89a00)] shadow-[inset_-4px_-6px_10px_rgba(180,100,0,0.35),0_6px_12px_-6px_rgba(150,90,0,0.6)]",
        className,
      )}
    />
  );
}

export function PhotoPlaceholder({ label, className }: { label: string; className?: string }) {
  return (
    <div
      className={cn(
        "grid place-items-center rounded-xl bg-lav-deep/60 text-[10px] font-semibold uppercase tracking-[0.14em] text-ink/40",
        className,
      )}
    >
      {label}
    </div>
  );
}
