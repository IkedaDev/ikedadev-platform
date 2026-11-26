export type SkeletonVariant = 'line' | 'line-sm' | 'line-lg' | 'circle' | 'card';

export interface SkeletonProps {
  variant?: SkeletonVariant;
  className?: string;
}

export function Skeleton({ variant = 'line', className = '' }: SkeletonProps) {
  return (
    <span
      className={['skeleton', `skeleton-${variant}`, className].filter(Boolean).join(' ')}
      aria-hidden="true"
    />
  );
}
