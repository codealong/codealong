// A value as the latest render has it, for what reads it later than the render it was made in: a
// listener registered once, a timer, a callback a child keeps, a promise that settles after the
// window moved on. It's set during render, as the windows always set these, so a key pressed
// before the next effect runs still sees what's on screen.

import { useRef } from 'react';

export function useLatest<T>(value: T): { readonly current: T } {
  const ref = useRef(value);
  ref.current = value;
  return ref;
}
