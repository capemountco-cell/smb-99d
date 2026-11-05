import { Check, Menu, ShoppingBag, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Sheet, SheetClose, SheetContent, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { cn } from "@/lib/utils";

import { CtaButton, Gummy, PhotoPlaceholder, Pouch, Stars } from "./primitives";

const NAV = [
  { href: "#buy", label: "Shop" },
  { href: "#why", label: "Why it works" },
  { href: "#how", label: "How it works" },
  { href: "#study", label: "The study" },
  { href: "#advisors", label: "Medical advisory board" },
  { href: "#reviews", label: "Reviews" },
];

export function TopBar({ cartCount }: { cartCount: number }) {
  return (
    <>
      <div className="bg-brand px-4 py-2 text-center text-[12px] font-semibold text-white">
        Save 55% on your first pouch · Free shipping over $50
      </div>
      <header className="sticky top-0 z-40 flex h-14 items-center justify-between border-b border-ink/5 bg-white/90 px-4 backdrop-blur-md">
        <Sheet>
          <SheetTrigger
            aria-label="Open menu"
            className="-ml-2 grid size-10 place-items-center rounded-full active:bg-lav"
          >
            <Menu className="size-6" strokeWidth={1.75} />
          </SheetTrigger>
          <SheetContent side="left" className="w-[82%] max-w-xs border-none p-0 [&>button]:hidden">
            <div className="flex h-14 items-center justify-between border-b px-5">
              <SheetTitle className="font-display text-2xl font-bold tracking-tight text-brand">
                hello nutra
              </SheetTitle>
              <SheetClose aria-label="Close menu" className="grid size-10 place-items-center">
                <X className="size-5" />
              </SheetClose>
            </div>
            <nav className="flex flex-col p-2">
              {NAV.map((n) => (
                <SheetClose asChild key={n.href}>
                  <a
                    href={n.href}
                    className="rounded-xl px-4 py-3.5 font-display text-lg font-semibold text-ink active:bg-lav"
                  >
                    {n.label}
                  </a>
                </SheetClose>
              ))}
            </nav>
          </SheetContent>
        </Sheet>
        <a href="#" className="font-display text-[26px] font-bold tracking-[-0.03em] text-brand">
          hello nutra
        </a>
        <a
          href="#buy"
          aria-label={`Cart, ${cartCount} items`}
          className="relative -mr-2 grid size-10 place-items-center rounded-full active:bg-lav"
        >
          <ShoppingBag className="size-6" strokeWidth={1.75} />
          {cartCount > 0 && (
            <span className="absolute right-1 top-1 grid size-4 place-items-center rounded-full bg-brand text-[10px] font-bold text-white">
              {cartCount}
            </span>
          )}
        </a>
      </header>
    </>
  );
}

const SLIDES = ["Pouch + pack", "Placeholder", "Placeholder", "Placeholder"];

function Gallery() {
  const [active, setActive] = useState(0);
  const track = useRef<HTMLDivElement>(null);

  const go = (i: number) => {
    const el = track.current;
    if (el) el.scrollTo({ left: i * el.clientWidth, behavior: "smooth" });
  };

  return (
    <div>
      <div
        ref={track}
        onScroll={(e) => {
          const el = e.currentTarget;
          setActive(Math.round(el.scrollLeft / el.clientWidth));
        }}
        className="no-scrollbar flex snap-x snap-mandatory overflow-x-auto"
      >
        <div className="relative aspect-square w-full shrink-0 snap-center overflow-hidden bg-lav">
          <ClinicianBadge className="absolute left-4 top-4 z-10" />
          <Pouch className="absolute left-1/2 top-[12%] -translate-x-1/2 -rotate-2" />
          <div className="absolute bottom-[5%] left-[5%] flex items-end">
            <Gummy className="size-11 rotate-[-12deg]" />
            <Gummy className="-ml-3 size-13 rotate-6" />
            <Gummy className="-ml-4 mb-1 size-10 rotate-[20deg]" />
          </div>
          <div className="absolute bottom-[10%] right-[5%] rotate-[8deg] rounded-md bg-white p-1.5 shadow-lg">
            <Pouch small className="rounded-sm shadow-none ring-0" />
          </div>
        </div>
        {SLIDES.slice(1).map((s, i) => (
          <div key={i} className="aspect-square w-full shrink-0 snap-center bg-lav p-5">
            <PhotoPlaceholder label={`Product photo ${i + 2}`} className="size-full" />
          </div>
        ))}
      </div>
      <div className="flex gap-2 px-5 pt-3">
        {SLIDES.map((s, i) => (
          <button
            key={i}
            onClick={() => go(i)}
            aria-label={`Show image ${i + 1}`}
            className={cn(
              "grid size-[60px] place-items-center rounded-lg bg-lav px-1 text-center text-[9px] leading-tight text-ink/60 transition",
              active === i ? "ring-2 ring-ink" : "ring-1 ring-transparent",
            )}
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}

export function ClinicianBadge({ className, small }: { className?: string; small?: boolean }) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center rounded-xl bg-white text-center shadow-sm",
        small ? "size-14 p-1" : "size-[76px] p-2",
        className,
      )}
    >
      <span className={cn("leading-none text-sun", small ? "text-xs" : "text-base")}>❦</span>
      <span
        className={cn(
          "font-display font-bold leading-[1.05] text-ink",
          small ? "text-[8px]" : "text-[11px]",
        )}
      >
        Clinicians'
        <br />
        Choice
      </span>
      <span className={cn("mt-0.5 text-ink/50", small ? "text-[5px]" : "text-[7px]")}>
        Recommended
      </span>
    </div>
  );
}

function useCountdown() {
  const [state, setState] = useState<{ left: string; date: string } | null>(null);
  useEffect(() => {
    const tick = () => {
      const now = new Date();
      const cutoff = new Date(now);
      cutoff.setHours(14, 0, 0, 0);
      if (cutoff <= now) cutoff.setDate(cutoff.getDate() + 1);
      const diff = Math.floor((cutoff.getTime() - now.getTime()) / 1000);
      const pad = (n: number) => String(n).padStart(2, "0");
      const left = `${pad(Math.floor(diff / 3600))}:${pad(Math.floor((diff % 3600) / 60))}:${pad(diff % 60)}`;
      const arrive = new Date(cutoff);
      arrive.setDate(arrive.getDate() + 6);
      const date = arrive.toLocaleDateString("en-US", {
        weekday: "long",
        month: "short",
        day: "numeric",
      });
      setState({ left, date });
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, []);
  return state;
}

type Plan = "subscribe" | "once";

function PlanOption({
  value,
  selected,
  onSelect,
  children,
}: {
  value: Plan;
  selected: boolean;
  onSelect: (p: Plan) => void;
  children: React.ReactNode;
}) {
  return (
    <label
      className={cn(
        "relative flex cursor-pointer items-center gap-3 rounded-2xl border bg-white p-4 transition",
        selected ? "border-ink border-[1.5px] shadow-sm" : "border-ink/15",
      )}
    >
      <input
        type="radio"
        name="plan"
        value={value}
        checked={selected}
        onChange={() => onSelect(value)}
        className="peer sr-only"
      />
      <span
        className={cn(
          "grid size-5 shrink-0 place-items-center rounded-full border-[1.5px] peer-focus-visible:ring-2 peer-focus-visible:ring-brand",
          selected ? "border-ink" : "border-ink/40",
        )}
      >
        {selected && <span className="size-2.5 rounded-full bg-ink" />}
      </span>
      {children}
    </label>
  );
}

const PERKS = [
  "Free shipping",
  "Skip, pause, or cancel anytime",
  "90-day money-back guarantee",
  "Discreet packaging",
];

const PAYMENTS = [
  "American Express",
  "Apple Pay",
  "Diners Club",
  "Discover",
  "Google Pay",
  "Mastercard",
  "Shop Pay",
  "Visa",
];

export function Hero({ onAdd }: { onAdd: () => void }) {
  const [plan, setPlan] = useState<Plan>("subscribe");
  const countdown = useCountdown();

  return (
    <section id="buy" className="scroll-mt-14 pb-10">
      <Gallery />

      <div className="px-5 pt-6">
        <a href="#reviews" className="flex items-center gap-2 text-sm">
          <Stars size={16} />
          <span className="font-semibold">4.9</span>
          <span className="text-ink/60 underline underline-offset-2">(27 reviews)</span>
        </a>
        <h1 className="mt-3 font-display text-[38px] font-bold leading-[0.98] tracking-[-0.03em] text-ink">
          Vaginal Probiotic Gummies
        </h1>
        <p className="mt-3 text-[15px] font-medium text-ink/80">
          For healthy odor, pH and urinary support*
        </p>
        <div className="mt-4 flex flex-wrap gap-2">
          {["Clinically studied probiotic¹", "<1 g sugar", "Pineapple flavor"].map((c) => (
            <span
              key={c}
              className="rounded-full bg-sun-soft px-3 py-1 text-[12px] font-semibold text-ink"
            >
              {c}
            </span>
          ))}
        </div>

        <fieldset className="mt-6 space-y-3">
          <legend className="sr-only">Choose a purchase option</legend>
          <PlanOption value="subscribe" selected={plan === "subscribe"} onSelect={setPlan}>
            <span className="absolute -top-2.5 left-4 rounded-md bg-sun px-2 py-0.5 text-[10px] font-extrabold uppercase tracking-wide text-ink">
              Save 55%
            </span>
            <span className="flex-1">
              <span className="block font-display text-[17px] font-semibold">
                Subscribe and save
              </span>
              <span className="mt-0.5 block text-[12px] leading-snug text-ink/60">
                Refill ships in 4 weeks · skip, pause or cancel anytime
              </span>
            </span>
            <span className="text-right">
              <span className="block font-display text-xl font-bold">$49.95</span>
              <span className="block text-xs text-ink/50 line-through">$111</span>
            </span>
          </PlanOption>
          <PlanOption value="once" selected={plan === "once"} onSelect={setPlan}>
            <span className="flex-1">
              <span className="block font-display text-[17px] font-semibold">Buy once</span>
              <span className="mt-0.5 block text-[12px] text-ink/60">$111.00 + $7.95 shipping</span>
            </span>
            <span className="font-display text-xl font-bold">$111</span>
          </PlanOption>
        </fieldset>

        <p className="mt-3 text-[12px] text-ink/60">
          {plan === "subscribe" ? "$1.78" : "$3.96"} a day · 28 daily packs (84 gummies)
        </p>

        <button
          onClick={onAdd}
          className="mt-4 flex h-14 w-full items-center justify-center rounded-2xl bg-brand font-display text-lg font-bold text-white shadow-[0_8px_24px_-8px] shadow-brand/60 transition active:scale-[0.98]"
        >
          Try it now
        </button>
        <p className="mt-2.5 text-center text-[11px] text-ink/60">
          {plan === "subscribe" ? "Discount auto-applied · " : ""}
          Order in{" "}
          <span className="font-semibold tabular-nums text-ink">
            {countdown?.left ?? "--:--:--"}
          </span>{" "}
          to get it by {countdown?.date ?? "…"}
        </p>

        <ul className="mt-5 grid grid-cols-2 gap-x-4 gap-y-2.5">
          {PERKS.map((p) => (
            <li key={p} className="flex gap-1.5 text-[13px] font-semibold leading-snug">
              <Check className="mt-0.5 size-3.5 shrink-0 text-brand" strokeWidth={3} />
              {p}
            </li>
          ))}
        </ul>

        <a
          href="#advisors"
          className="mt-6 flex items-center gap-3 rounded-2xl bg-sun-soft p-3 active:bg-sun/40"
        >
          <ClinicianBadge small />
          <span className="text-[13px] leading-snug">
            <span className="block font-bold">Reviewed by our medical advisory board</span>
            <span className="text-ink/70">4 clinicians · </span>
            <span className="font-semibold underline underline-offset-2">View clinicians</span>
          </span>
        </a>

        <p className="mt-5 text-[11px] leading-relaxed text-ink/60">
          Refill ships in 4 weeks · Try risk-free for 90 days or 100% money back · Less than 1% of
          customers use our money-back guarantee
        </p>
        <ul className="mt-3 flex flex-wrap gap-1.5" aria-label="Accepted payment methods">
          {PAYMENTS.map((p) => (
            <li
              key={p}
              className="rounded-md border border-ink/15 px-2 py-1 text-[10px] font-medium text-ink/60"
            >
              {p}
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
