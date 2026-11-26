import { useEffect } from 'react';

let activeLocks = 0;

export function useBodyScrollLock(locked: boolean): void {
  useEffect(() => {
    if (!locked || typeof document === 'undefined') return;
    activeLocks += 1;
    document.body.classList.add('scroll-locked');

    return () => {
      activeLocks = Math.max(0, activeLocks - 1);
      if (activeLocks === 0) {
        document.body.classList.remove('scroll-locked');
      }
    };
  }, [locked]);
}
