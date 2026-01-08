import type { Accessor } from "solid-js";

export default function Selection<
  T = string | number | string[] | undefined
>(props: {
  id: string;
  options: Accessor<[T, string, boolean?][]>;
  onSelection?: (value: T) => void;
  value: Accessor<T>;
}) {
  var selected = null as T;

  const clickHandler = (e: Event) => {
    if (selected !== e.target?.value) {
      selected = e.target?.value;
      console.log(selected);
      if (props.onSelection) {
        props.onSelection(selected);
      }
    }
  };

  const changeHandler = (e: Event) => {
    if (selected !== e.target?.value) {
      selected = e.target?.value;
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
      >
        {props.options().map(([value, label, disabled]) => (
          <option value={value} disabled={disabled}>
            {label}
          </option>
        ))}
      </select>
    </>
  );
}
