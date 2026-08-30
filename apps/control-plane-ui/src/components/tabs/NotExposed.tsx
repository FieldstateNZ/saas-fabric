/**
 * A tab that has nothing to show yet, and says exactly why.
 *
 * # Why this exists rather than hiding the tab
 *
 * The console's job in this increment is to make the platform legible,
 * including the parts of it that are not wired up. A tab that quietly
 * disappears until its API arrives leaves an operator unable to tell "SaaS
 * Fabric does not do this" from "SaaS Fabric does this and I cannot see it" —
 * and those need very different conversations.
 *
 * Naming the missing API here is also how the next piece of work gets chosen:
 * the screen states the requirement, rather than a backlog guessing at it.
 */
export function NotExposed({ shows, needs }: { shows: string; needs: string }) {
  return (
    <section className="not-exposed">
      <p className="not-exposed__shows">This tab will show {shows}.</p>
      <p className="not-exposed__needs">
        <strong>Not exposed yet.</strong> {needs}
      </p>
    </section>
  )
}
