import { For, type Accessor } from "solid-js";

export default function Selection<
  T extends string | number | string[] | undefined
>(props: {
  id: string;
  options: Accessor<[T, string, boolean?][]>;
  onSelection?: (value: T) => void;
  value: Accessor<T>;
  disabled?: Accessor<boolean>;
}) {
  var selected = undefined as T;

  const clickHandler = (e: Event) => {
    const newValue = (e.target as HTMLSelectElement)?.value as T;
    if (selected !== newValue) {
      selected = newValue;
      console.log(selected);
      if (props.onSelection) {
        props.onSelection(selected);
      }
    }
  };

  const changeHandler = (e: Event) => {
    const newValue = (e.target as HTMLSelectElement)?.value as T;
    if (selected !== newValue) {
      selected = newValue;
      if (props.onSelection) {
        props.onSelection(selected);
      }
    }
  };

  return (
    <>
      <select
        name={props.id}
        id={props.id}
        on:click={clickHandler}
        on:change={changeHandler}
        value={props.value()}
        disabled={props.disabled ? props.disabled() : false}
      >
        <For each={props.options()}>
          {([value, label, disabled]) => (
            <option value={value} disabled={disabled}>
              {label}
            </option>
          )}
        </For>
      </select>
    </>
  );
}
