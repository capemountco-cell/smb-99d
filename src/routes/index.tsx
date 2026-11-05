import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "Starter Project" },
      { name: "description", content: "An empty starter project." },
      { property: "og:title", content: "Starter Project" },
      { property: "og:description", content: "An empty starter project." },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary" },
    ],
  }),
  component: Index,
});

function Index() {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background">
      <h1 className="text-2xl font-semibold text-foreground">Starter Project</h1>
    </main>
  );
}
