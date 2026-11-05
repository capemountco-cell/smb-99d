import { useEffect, useRef, useState } from "react";

import { cn } from "@/lib/utils";

import { CtaButton, Eyebrow, Heading, Section, Stars } from "./primitives";

function useInView<T extends Element>() {
  const ref = useRef<T>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof IntersectionObserver === "undefined") {
      setSeen(true);
      return;
    }
    const io = new IntersectionObserver(
      ([e]) => {
        if (e?.isIntersecting) {
          setSeen(true);
          io.disconnect();
        }
      },
      { threshold: 0.3 },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);
  return [ref, seen] as const;
}

const STUDY_STATS = [
  { v: "45+", l: "years of research" },
  { v: "25+", l: "published studies" },
  { v: "120", l: "women, 8-week study" },
  { v: "2.6x", l: "more women saw a healthier odor than placebo*" },
];

const BARS = [
  { label: "Odor", p: 62, c: 24 },
  { label: "pH", p: 48, c: 19 },
  { label: "Flora", p: 55, c: 21 },
];

export function Study() {
  const [ref, seen] = useInView<HTMLDivElement>();
  return (
    <Section id="study" className="bg-lav/60 pt-2">
      <div className="rounded-3xl bg-ink p-6 text-white">
        <p className="text-[11px] font-bold uppercase tracking-[0.14em] text-sun">The study</p>
        <h2 className="mt-2 font-display text-[30px] font-bold leading-[1.05] tracking-[-0.02em]">
          Includes Clinically Studied Probiotics
        </h2>
        <p className="mt-3 text-[14px] leading-relaxed text-white/75">
          The study: the probiotic we selected for every pack, SNZ 1969®, was tested in a
          placebo-controlled clinical study of 120 women over 8 weeks.
        </p>
        <dl className="mt-6 grid grid-cols-2 gap-x-4 gap-y-5">
          {STUDY_STATS.map((s) => (
            <div key={s.v}>
              <dt className="font-display text-[42px] font-bold leading-none text-sun">{s.v}</dt>
              <dd className="mt-1.5 text-[12px] leading-snug text-white/75">{s.l}</dd>
            </div>
          ))}
        </dl>
      </div>

      <div ref={ref} className="mt-6 space-y-5 rounded-3xl bg-white p-5">
        {BARS.map((b) => (
          <div key={b.label}>
            <p className="text-[13px] font-bold">
              {b.label}: Probiotic {b.p}% · Placebo {b.c}%
            </p>
            <div className="mt-2 space-y-1.5" aria-hidden>
              <div className="h-2.5 overflow-hidden rounded-full bg-lav">
                <div
                  className="h-full rounded-full bg-brand transition-[width] duration-1000 ease-out"
                  style={{ width: seen ? `${b.p}%` : "0%" }}
                />
              </div>
              <div className="h-2.5 overflow-hidden rounded-full bg-lav">
                <div
                  className="h-full rounded-full bg-ink/25 transition-[width] delay-150 duration-1000 ease-out"
                  style={{ width: seen ? `${b.c}%` : "0%" }}
                />
              </div>
            </div>
          </div>
        ))}
        <div className="flex gap-4 pt-1 text-[11px] text-ink/60">
          <span className="flex items-center gap-1.5">
            <span className="size-2.5 rounded-full bg-brand" /> Probiotic
          </span>
          <span className="flex items-center gap-1.5">
            <span className="size-2.5 rounded-full bg-ink/25" /> Placebo
          </span>
        </div>
      </div>

      <p className="mt-4 text-[11px] leading-relaxed text-ink/55">
        *Share of women reporting each result after 8 weeks. 62% vs 24% odor, 48% vs 19% pH, 55% vs
        21% flora, placebo-controlled study of 120 women. SNZ 1969® has 45+ years of research and
        25+ published studies.
      </p>
      <p className="mt-4 text-[13px] font-bold">Formula reviewed by our medical advisory board</p>
      <CtaButton variant="ink" className="mt-4" />
      <p className="mt-2.5 text-center text-[11px] text-ink/50">Individual results vary.</p>
    </Section>
  );
}

const ADVISORS = [
  {
    name: "Dr. Mark Matus",
    role: "MD · Physician, preventive medicine and metabolic health",
    quote:
      "Quality and ingredient integrity matter to me as a doctor. HelloNutra's commitment to clear labeling, smart formulations, and ingredient transparency makes it a brand I trust and feel comfortable recommending.",
    tone: "bg-[oklch(0.9_0.03_240)]",
  },
  {
    name: "Stephanie Wright",
    role: "RN, BSN · Registered nurse",
    quote:
      "HelloNutra creates supplements that are practical, enjoyable to take, and designed to support daily habits over time.",
    tone: "bg-[oklch(0.92_0.03_60)]",
  },
  {
    name: "Dr. Ryan Whitfield",
    role: "MD · Public health physician",
    quote:
      "HelloNutra's science-driven formulations and transparency support a practical, long-term approach to everyday wellness.",
    tone: "bg-[oklch(0.9_0.02_200)]",
  },
  {
    name: "Katie Ernst",
    role: "RD · Registered dietitian",
    quote:
      "HelloNutra's simple formulations, transparent ingredient explanations, and clear 'who this is for' guidance make it easier for people to choose supplements that actually fit their needs.",
    tone: "bg-[oklch(0.9_0.04_140)]",
  },
];

function initials(name: string) {
  return name
    .replace("Dr. ", "")
    .split(" ")
    .map((p) => p[0])
    .join("");
}

export function Advisors() {
  return (
    <Section id="advisors">
      <Eyebrow>Medical advisory board</Eyebrow>
      <Heading className="mt-2">Our medical advisory board</Heading>
      <p className="mt-3 text-[15px] leading-relaxed text-ink/75">
        Our formula is reviewed by a medical advisory board of physicians, a registered dietitian
        and a registered nurse.
      </p>

      <div className="mt-6 grid grid-cols-2 gap-x-3 gap-y-6">
        {ADVISORS.map((a) => (
          <figure key={a.name}>
            <div
              className={cn(
                "grid aspect-[4/5] place-items-center rounded-2xl font-display text-4xl font-bold text-ink/30",
                a.tone,
              )}
              aria-label={`Photo of ${a.name}`}
            >
              {initials(a.name)}
            </div>
            <figcaption className="mt-3">
              <span className="block font-display text-[15px] font-bold leading-tight">
                {a.name}
              </span>
              <span className="mt-0.5 block text-[10.5px] leading-snug text-ink/55">{a.role}</span>
            </figcaption>
            <blockquote className="mt-2 text-[12px] leading-snug text-ink/75">
              “{a.quote}”
            </blockquote>
          </figure>
        ))}
      </div>

      <CtaButton className="mt-8" />
      <p className="mt-4 text-[10.5px] leading-relaxed text-ink/50">
        Disclosure: Our advisors are paid consultants to HelloNutra. Their endorsements are based on
        a review of our ingredient formulation and manufacturing standards and are not medical
        advice.
      </p>
    </Section>
  );
}

const SAYING = [
  {
    t: "Freshness",
    q: "I don't think about odor anymore. I just feel fresh all day.",
    n: "Megan L.",
  },
  { t: "Taste", q: "It tastes like pineapple candy. Three gummies and I'm set.", n: "Priya S." },
  {
    t: "Confidence",
    q: "I finally feel like myself again, even on long days.",
    n: "Courtney B.",
  },
  { t: "Routine", q: "Three gummies with my coffee. Done.", n: "Dana R." },
  {
    t: "No more wipes",
    q: "I threw out my wipes and sprays. One daily pack does it.",
    n: "Ashley V.",
  },
  { t: "Discreet", q: "Nobody knows it's a probiotic. It just looks like a snack.", n: "Erin G." },
];

export function Saying() {
  return (
    <Section className="bg-brand-soft">
      <Eyebrow>What women are saying</Eyebrow>
      <Heading className="mt-2">Real women, real routines</Heading>
      <ul className="mt-6 divide-y divide-brand/15">
        {SAYING.map((s) => (
          <li key={s.t} className="py-4 text-[14px] leading-relaxed">
            <span className="mb-1 flex items-center gap-2">
              <span className="font-display font-bold">{s.t}</span>
              <Stars size={12} />
            </span>
            “{s.q}” <span className="text-ink/55">{s.n}</span>
          </li>
        ))}
      </ul>
    </Section>
  );
}

const SURVEY = [
  { v: "91%", l: "Felt fresher throughout the day*" },
  { v: "89%", l: "Said it was easy to take every day*" },
  { v: "87%", l: "Felt more confident*" },
  { v: "84%", l: "Worried less about odor*" },
];

export function Survey() {
  return (
    <Section>
      <Eyebrow>Customer survey</Eyebrow>
      <Heading className="mt-2">Women feel the difference</Heading>
      <p className="mt-3 text-[15px] font-medium">
        “I noticed a difference by week 3.” <span className="text-ink/55">Tasha W.</span>
      </p>
      <p className="mt-3 text-[15px] leading-relaxed text-ink/75">
        We asked HelloNutra customers how they felt taking 3 gummies a day. Here's what they told
        us.
      </p>
      <dl className="mt-6 grid grid-cols-2 gap-3">
        {SURVEY.map((s) => (
          <div key={s.v} className="rounded-2xl bg-lav p-4">
            <dt className="font-display text-[44px] font-bold leading-none tracking-tight text-brand">
              {s.v}
            </dt>
            <dd className="mt-2 text-[12px] leading-snug text-ink/75">{s.l}</dd>
          </div>
        ))}
      </dl>
      <p className="mt-3 text-[11px] text-ink/50">
        *Based on a survey of 27 HelloNutra customers. Individual results vary.
      </p>
      <CtaButton className="mt-6" />
    </Section>
  );
}

const TOPICS = ["Taste", "Freshness", "Confidence", "Easy routine", "On the go", "Comfort"];

const REVIEWS = [
  {
    topic: "Taste",
    title: "Tastes like pineapple candy!",
    body: "I honestly forget these are a probiotic. They taste like pineapple candy and I look forward to them every morning.",
    n: "Brianna K.",
  },
  {
    topic: "Taste",
    title: "Finally a supplement I don't have to force down",
    body: "No chalky aftertaste, just bright pineapple flavor. My roommate keeps asking what candy I'm eating.",
    n: "Sofia L.",
  },
  {
    topic: "Taste",
    title: "Better than the gummies I buy for fun",
    body: "Soft, chewy and actually delicious. Three a day is not a hardship at all.",
    n: "Taylor B.",
  },
  {
    topic: "Freshness",
    title: "I stopped worrying about it",
    body: "That odor that would come and go after my period? I haven't noticed it in weeks.",
    n: "Megan L.",
  },
  {
    topic: "Freshness",
    title: "Fresh even after spin class",
    body: "I work out five days a week and still feel fresh by the end of the day.",
    n: "Jess M.",
  },
  {
    topic: "Confidence",
    title: "I feel like myself again",
    body: "I didn't realize how much I was thinking about it until I stopped. Huge confidence boost.",
    n: "Courtney B.",
  },
  {
    topic: "Easy routine",
    title: "Easiest habit I have",
    body: "One pack next to the coffee maker. I haven't missed a day in two months.",
    n: "Dana R.",
  },
  {
    topic: "On the go",
    title: "Lives in my bag",
    body: "The daily packs are perfect for travel. No bottle, no water, no fuss.",
    n: "Jordan P.",
  },
  {
    topic: "Comfort",
    title: "So much more comfortable",
    body: "Less irritation and I've ditched the scented washes entirely.",
    n: "Ashley V.",
  },
];

export function Reviews() {
  const [topic, setTopic] = useState("Taste");
  const list = REVIEWS.filter((r) => r.topic === topic);
  return (
    <Section id="reviews">
      <Eyebrow>Reviews by topic</Eyebrow>
      <Heading className="mt-2">Join the women who feel fresh every day</Heading>
      <p className="mt-3 flex items-center gap-2 text-[14px]">
        <Stars size={14} />
        <span>
          <b>4.9</b> from 27 reviews
        </span>
      </p>
      <div
        role="tablist"
        aria-label="Review topics"
        className="no-scrollbar -mx-5 mt-5 flex gap-2 overflow-x-auto px-5"
      >
        {TOPICS.map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={topic === t}
            onClick={() => setTopic(t)}
            className={cn(
              "shrink-0 rounded-full border px-4 py-2 text-[13px] font-semibold transition",
              topic === t ? "border-ink bg-ink text-white" : "border-ink/20 bg-white text-ink",
            )}
          >
            {t}
          </button>
        ))}
      </div>
      <div className="mt-5 space-y-3" role="tabpanel">
        {list.map((r) => (
          <article key={r.title} className="rounded-2xl border border-ink/10 p-4">
            <Stars size={12} />
            <h3 className="mt-2 font-display text-[17px] font-bold leading-tight">{r.title}</h3>
            <p className="mt-2 text-[13.5px] leading-relaxed text-ink/70">{r.body}</p>
            <p className="mt-3 text-[12px] font-semibold">
              {r.n} <span className="font-normal text-ink/50">· Verified buyer</span>
            </p>
          </article>
        ))}
      </div>
    </Section>
  );
}

export function Footer() {
  return (
    <footer className="space-y-3 border-t border-ink/10 bg-lav/60 px-5 pb-28 pt-8 text-[10.5px] leading-relaxed text-ink/55">
      <p className="font-display text-2xl font-bold tracking-tight text-brand">hello nutra</p>
      <p>
        ¹SNZ 1969®, a trademark of Sanzyme Biologics, is clinically studied to support a healthy
        vaginal odor, pH balance and microbiome.
      </p>
      <p>
        *These statements have not been evaluated by the Food and Drug Administration. This product
        is not intended to diagnose, treat, cure, or prevent any disease.
      </p>
      <p>© {new Date().getFullYear()} hello nutra</p>
    </footer>
  );
}
