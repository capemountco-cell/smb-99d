import * as Accordion from "@radix-ui/react-accordion";
import { Droplet, Play, Plus, Shield, Sparkles } from "lucide-react";
import { useState } from "react";

import { Eyebrow, Heading, PhotoPlaceholder, Pouch, Section, Stars } from "./primitives";

const BENEFIT_TILES = [
  { icon: Droplet, label: "Balances pH & yeast*" },
  { icon: Sparkles, label: "Promotes healthy odor*" },
  { icon: Shield, label: "Supports flora & urinary tract*" },
];

const FAQ = [
  {
    q: "What makes HelloNutra different?",
    a: "Most gummies use generic strains. HelloNutra is built around SNZ 1969®, a probiotic tested in a placebo-controlled clinical study of women, plus cranberry and D-Mannose for urinary support — all in a pineapple gummy with under 1 g of sugar.",
  },
  {
    q: "Benefits",
    a: "Supports healthy vaginal odor and pH, helps maintain a balanced vaginal microbiome and yeast levels, and supports urinary tract health.* Vitamin C adds antioxidant support.",
  },
  {
    q: "How does it work?",
    a: "Good bacteria help keep vaginal pH in the healthy 3.8–4.5 range. Periods, sex, workouts and stress can push pH up. A daily dose of clinically studied probiotic helps good bacteria stay in the lead.*",
  },
  {
    q: "What's inside?",
    a: "SNZ 1969® probiotic, a prebiotic, cranberry, D-Mannose, vitamin C, pineapple and aloe. No gelatin, no artificial colors, and under 1 g of sugar per serving.",
  },
  {
    q: "How do I take it?",
    a: "Chew all three gummies from one daily pack, any time of day — with or without food. Consistency matters most, so many women pair it with their morning coffee.",
  },
  {
    q: "What results can I expect?",
    a: "Many customers say they notice a difference within 2–3 weeks. In the clinical study, results were measured after 8 weeks of daily use. Individual results vary.",
  },
];

export function Problem() {
  return (
    <Section id="why" className="pt-4">
      <Eyebrow>Sound familiar?</Eyebrow>
      <Heading className="mt-2">Odor, pH and washes that make it worse.</Heading>
      <p className="mt-4 text-[15px] leading-relaxed text-ink/75">
        A doctor-reviewed vaginal probiotic with a clinically studied probiotic. Plus a prebiotic,
        cranberry and D-Mannose for urinary support, vitamin C for antioxidant support, and
        pineapple and aloe for daily feminine wellness.* Three delicious gummies in grab-and-go
        daily packs make it easy to stay consistent wherever you go.
      </p>
      <p className="mt-4 text-[15px] font-semibold leading-relaxed text-brand">
        You've noticed odor that comes and goes · Your pH feels off after your period, sex or the
        gym · You're tired of washes, wipes and sprays · You want urinary support* in the same daily
        habit
      </p>

      <div className="mt-6 grid grid-cols-3 gap-2.5">
        {BENEFIT_TILES.map(({ icon: Icon, label }) => (
          <div
            key={label}
            className="flex flex-col items-center gap-2 rounded-2xl bg-brand-soft px-2 py-4 text-center"
          >
            <Icon className="size-6 text-brand" strokeWidth={1.75} />
            <span className="text-[11.5px] font-semibold leading-tight">{label}</span>
          </div>
        ))}
      </div>

      <Accordion.Root type="single" collapsible className="mt-8">
        {FAQ.map((f) => (
          <Accordion.Item key={f.q} value={f.q} className="border-b border-ink/10">
            <Accordion.Header>
              <Accordion.Trigger className="group flex w-full items-center justify-between py-4 text-left font-display text-[17px] font-semibold">
                {f.q}
                <Plus
                  className="size-5 shrink-0 text-brand transition-transform duration-200 group-data-[state=open]:rotate-45"
                  strokeWidth={2.25}
                />
              </Accordion.Trigger>
            </Accordion.Header>
            <Accordion.Content className="overflow-hidden text-[14px] leading-relaxed text-ink/70 data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down">
              <p className="pb-4">{f.a}</p>
            </Accordion.Content>
          </Accordion.Item>
        ))}
      </Accordion.Root>
    </Section>
  );
}

const QUICK_QUOTES = [
  { q: "Finally something that works.", n: "Jess M." },
  { q: "No more guessing how I'll feel by myself.", n: "Brianna K." },
  { q: "Easiest part of my morning.", n: "Dana R." },
  { q: "My go-to after the gym.", n: "Courtney B." },
];

const CUSTOMER_PHOTOS = [
  {
    q: "I stopped thinking about it all day. One pack in the morning and I feel fresh until bedtime.",
    n: "Kendra T.",
  },
  { q: "It honestly tastes like pineapple candy. I look forward to it.", n: "Alicia M." },
  { q: "One pack in my bag and I'm done. No bottle, no water.", n: "Jordan P." },
  { q: "Three gummies every morning, that's it. Easiest habit I have.", n: "Renee S." },
];

export function RealWomen() {
  return (
    <div className="bg-lav/60 pb-10 pt-6">
      <div className="no-scrollbar flex snap-x snap-mandatory gap-3 overflow-x-auto scroll-px-5 px-5 pb-2">
        {QUICK_QUOTES.map((t) => (
          <figure
            key={t.n}
            className="w-[72%] shrink-0 snap-start rounded-2xl bg-white p-4 shadow-[0_1px_0_rgba(0,0,0,0.04)]"
          >
            <Stars />
            <blockquote className="mt-2 font-display text-[16px] font-semibold leading-snug">
              “{t.q}”
            </blockquote>
            <figcaption className="mt-2 text-xs text-ink/50">{t.n}</figcaption>
          </figure>
        ))}
      </div>

      <div className="px-5 pt-8">
        <Eyebrow>Real women</Eyebrow>
        <Heading className="mt-2">From women like us</Heading>
        <div className="mt-5 grid grid-cols-2 gap-3">
          {CUSTOMER_PHOTOS.map((c) => (
            <figure key={c.n} className="rounded-2xl bg-white p-2.5 pb-3.5">
              <PhotoPlaceholder label="Customer photo" className="aspect-[4/5]" />
              <Stars className="mt-3 px-1" size={12} />
              <blockquote className="mt-1.5 px-1 text-[12.5px] leading-snug text-ink/80">
                “{c.q}”
              </blockquote>
              <figcaption className="mt-1.5 px-1 text-[12px] font-bold">{c.n}</figcaption>
            </figure>
          ))}
        </div>
      </div>
    </div>
  );
}

export function VideoBlock() {
  const [playing, setPlaying] = useState(false);
  return (
    <div className="px-5 py-10">
      <button
        onClick={() => setPlaying(true)}
        aria-label="Play video"
        className="relative grid aspect-[4/5] w-full place-items-center overflow-hidden rounded-3xl bg-sun-soft"
      >
        <Pouch className="scale-90" />
        <span className="absolute grid size-16 place-items-center rounded-full bg-ink text-white shadow-xl transition active:scale-95">
          <Play className="ml-1 size-7 fill-current" />
        </span>
        {playing && (
          <span className="absolute inset-x-6 bottom-6 rounded-xl bg-ink/85 px-4 py-3 text-sm text-white">
            Video coming soon
          </span>
        )}
      </button>
    </div>
  );
}

const DOTS_OFF = [
  [38, 60, "g"],
  [58, 92, "d"],
  [34, 118, "g"],
  [62, 130, "d"],
  [44, 150, "g"],
  [52, 176, "d"],
  [40, 100, "d"],
] as const;
const DOTS_ON = [
  [36, 64, "p"],
  [60, 84, "p"],
  [42, 104, "p"],
  [58, 122, "y"],
  [38, 140, "p"],
  [62, 158, "p"],
  [48, 180, "g"],
  [50, 132, "p"],
] as const;
const DOT_FILL = {
  g: "oklch(0.8 0.01 285)",
  d: "oklch(0.35 0.01 285)",
  p: "oklch(0.66 0.26 350)",
  y: "oklch(0.91 0.17 98)",
};

function Leaf({ dots }: { dots: ReadonlyArray<readonly [number, number, keyof typeof DOT_FILL]> }) {
  return (
    <svg viewBox="0 0 96 240" className="h-48 w-auto" aria-hidden>
      <path
        d="M48 6 C 92 70, 92 170, 48 234 C 4 170, 4 70, 48 6 Z"
        fill="white"
        stroke="oklch(0.2 0.01 285)"
        strokeWidth="2.5"
      />
      {dots.map(([x, y, c], i) => (
        <circle
          key={i}
          cx={x}
          cy={y}
          r={5}
          fill={DOT_FILL[c]}
          stroke={c === "y" ? "oklch(0.2 0.01 285)" : "none"}
          strokeWidth={1.5}
        />
      ))}
    </svg>
  );
}

const STEPS = [
  { n: 1, t: "Off balance: Unwanted bacteria crowd in · pH creeps up past 4.5" },
  { n: 2, t: "Balanced*: Good bacteria lead · pH stays in the 3.8 to 4.5 range" },
  { n: 3, t: "Fresh and comfortable*" },
];

export function HowItWorks() {
  return (
    <Section id="how" className="bg-lav/60">
      <Eyebrow>How it works</Eyebrow>
      <Heading className="mt-2">Same microbiome, two states.</Heading>

      <div className="mt-8 grid grid-cols-2 gap-4 text-center">
        <figure className="flex flex-col items-center">
          <Leaf dots={DOTS_OFF} />
          <figcaption className="mt-3">
            <span className="block text-[13px] font-bold">Off balance</span>
            <span className="text-[11px] text-ink/60">pH 5.2 · Unwanted bacteria lead</span>
          </figcaption>
        </figure>
        <figure className="flex flex-col items-center">
          <Leaf dots={DOTS_ON} />
          <figcaption className="mt-3">
            <span className="block text-[13px] font-bold">Balanced*</span>
            <span className="text-[11px] text-ink/60">pH 4.2 · Good bacteria lead</span>
          </figcaption>
        </figure>
      </div>

      <div className="mt-7">
        <div className="relative h-2.5 overflow-hidden rounded-full bg-white">
          <div
            className="absolute inset-y-0 rounded-full bg-brand"
            style={{ left: "15%", width: "35%" }}
          />
        </div>
        <div className="mt-1.5 flex justify-between text-[11px] text-ink/60">
          <span>pH 3.5</span>
          <span className="font-bold text-ink">Healthy range 3.8 to 4.5</span>
          <span>5.5</span>
        </div>
        <p className="mt-2 text-[11px] text-ink/50">
          Illustration. A healthy vaginal pH usually sits between 3.8 and 4.5.
        </p>
      </div>

      <p className="mt-5 text-[15px] leading-relaxed text-ink/75">
        Your vaginal microbiome is a living mix of bacteria. Periods, sex, the gym and even stress
        can throw it off.
      </p>

      <ol className="mt-6 grid grid-cols-3 gap-2.5">
        {STEPS.map((s) => (
          <li key={s.n} className="rounded-2xl bg-white p-3">
            <span className="font-display text-2xl font-bold text-brand">{s.n}</span>
            <p className="mt-1 text-[11.5px] leading-snug text-ink/80">{s.t}</p>
          </li>
        ))}
      </ol>
    </Section>
  );
}
