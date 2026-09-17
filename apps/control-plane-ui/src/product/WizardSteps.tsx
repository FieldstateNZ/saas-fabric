/**
 * The numbered step list `ClientForm` shows above its wizard.
 *
 * A plain ordered list rather than a control an operator can click ahead
 * with — `ClientForm` only advances a step at a time, on submit, so nothing
 * here is interactive; it exists to say where the operator is, not to let
 * them jump.
 */
export function WizardSteps({ steps, current }: { steps: readonly string[]; current: number }) {
  return (
    <ol className="wizard-steps">
      {steps.map((name, index) => (
        <li key={name} aria-current={current === index ? 'step' : undefined}>
          {index + 1}. {name}
        </li>
      ))}
    </ol>
  )
}
