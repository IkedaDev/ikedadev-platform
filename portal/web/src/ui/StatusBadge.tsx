export type StatusTone = 'emerald' | 'green' | 'blue' | 'amber' | 'red' | 'gray';

export interface StatusBadgeProps {
  label: string;
  tone?: StatusTone;
}

export function StatusBadge({ label, tone = 'gray' }: StatusBadgeProps) {
  return <span className={`badge badge-${tone}`}>{label}</span>;
}

export function formatMoney(amount: number, currency = 'CLP', locale = 'es-CL'): string {
  try {
    return new Intl.NumberFormat(locale, {
      style: 'currency',
      currency,
      maximumFractionDigits: currency === 'CLP' ? 0 : 2,
    }).format(amount);
  } catch {
    return `${amount} ${currency}`;
  }
}

export function formatDateTime(iso: string, locale = 'es-CL'): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date);
}
