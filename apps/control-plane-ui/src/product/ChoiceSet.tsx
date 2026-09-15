import { Check } from './Check'

/** What a {@link ChoiceSet} needs: a label, what can be chosen, and which of it is. */
interface ChoiceSetProps {
  readonly label: string
  readonly choices: readonly { id: string; name: string }[]
  readonly value: readonly string[]
  readonly onChange: (value: string[]) => void
}

/**
 * A multi-select built from checkboxes: which features implement something,
 * or which features a plan grants.
 *
 * A plain list of checkboxes rather than a multi-select input, because the
 * choices here are usually few and an operator scanning what a plan grants
 * benefits more from seeing every option at once than from a control that
 * hides most of them.
 */
export function ChoiceSet({ label, choices, value, onChange }: ChoiceSetProps) {
  return (
    <fieldset className="choice-set">
      <legend>{label}</legend>
      {choices.length === 0 && <p className="empty">No options defined yet.</p>}
      {choices.map((choice) => (
        <Check
          key={choice.id}
          label={choice.name || choice.id}
          value={value.includes(choice.id)}
          onChange={(checked) => {
            onChange(checked ? [...value, choice.id] : value.filter((id) => id !== choice.id))
          }}
        />
      ))}
    </fieldset>
  )
}
