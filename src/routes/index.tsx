import { createFileRoute } from "@tanstack/react-router";
import { Check } from "lucide-react";
import { useEffect, useState } from "react";

import { Hero, TopBar } from "@/components/landing/Hero";
import { Advisors, Footer, Reviews, Saying, Study, Survey } from "@/components/landing/Proof";
import { HowItWorks, Problem, RealWomen, VideoBlock } from "@/components/landing/Story";
import { cn } from "@/lib/utils";

const TITLE = "hello nutra · Vaginal Probiotic Gummies";
const DESCRIPTION =
  "Doctor-reviewed vaginal probiotic gummies for healthy odor, pH and urinary support. Save 55% on your first pouch.";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: TITLE },
      { name: "description", content: DESCRIPTION },
      { property: "og:title", content: TITLE },
      { property: "og:description", content: DESCRIPTION },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary" },
    ],
  }),
  component: Index,
});

function StickyBuyBar({ show, onAdd }: { show: boolean; onAdd: () => void }) {
  return (
    <div
      className={cn(
        "fixed inset-x-0 bottom-0 z-30 mx-auto max-w-[480px] transition-transform duration-300",
        show ? "translate-y-0" : "translate-y-full",
      )}
    >
      <div className="flex items-center gap-3 border-t border-ink/10 bg-white/95 px-4 pb-[max(12px,env(safe-area-inset-bottom))] pt-3 shadow-[0_-8px_24px_-12px_rgba(0,0,0,0.15)] backdrop-blur-md">
        <span className="holo flex h-12 w-10 shrink-0 items-center justify-center rounded-lg text-center font-display text-[10px] font-extrabold leading-[0.85] text-brand ring-1 ring-ink/5">
          hello
          <br />
          nutra
        </span>
        <div className="min-w-0 flex-1 leading-tight">
          <p className="truncate font-display text-[14px] font-bold">Vaginal Probiotic Gummies</p>
          <p className="text-[12px] text-ink/60">
            <b className="text-ink">$49.95</b> <s>$111</s> · Save 55%
          </p>
        </div>
        <button
          onClick={onAdd}
          className="h-11 shrink-0 rounded-xl bg-brand px-5 font-display text-[15px] font-bold text-white active:scale-[0.97]"
        >
          Try it now
        </button>
      </div>
    </div>
  );
}

function Index() {
  const [cart, setCart] = useState(0);
  const [toast, setToast] = useState(false);
  const [showBar, setShowBar] = useState(false);

  useEffect(() => {
    const onScroll = () => setShowBar(window.scrollY > 1100);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = setTimeout(() => setToast(false), 2200);
    return () => clearTimeout(id);
  }, [toast]);

  const add = () => {
    setCart((c) => c + 1);
    setToast(true);
  };

  return (
    <div className="min-h-screen bg-lav-deep/50">
      <main className="relative mx-auto min-h-screen max-w-[480px] overflow-x-clip bg-white text-ink shadow-[0_0_60px_-20px_rgba(0,0,0,0.2)]">
        <TopBar cartCount={cart} />
        <Hero onAdd={add} />
        <Problem />
        <RealWomen />
        <VideoBlock />
        <HowItWorks />
        <Study />
        <Advisors />
        <Saying />
        <Survey />
        <Reviews />
        <Footer />
      </main>

      <StickyBuyBar show={showBar} onAdd={add} />

      <div
        role="status"
        aria-live="polite"
        className={cn(
          "pointer-events-none fixed inset-x-0 top-16 z-50 mx-auto flex w-fit items-center gap-2 rounded-full bg-ink px-4 py-2.5 text-[13px] font-semibold text-white shadow-lg transition-all duration-300",
          toast ? "translate-y-0 opacity-100" : "-translate-y-3 opacity-0",
        )}
      >
        <Check className="size-4 text-sun" strokeWidth={3} />
        Added to your bag
      </div>
    </div>
  );
}
