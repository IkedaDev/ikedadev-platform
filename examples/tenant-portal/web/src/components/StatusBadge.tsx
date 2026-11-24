interface StatusBadgeProps {
  label: string;
  tone?: 'blue' | 'green' | 'amber' | 'red' | 'gray';
}

export function StatusBadge({ label, tone = 'gray' }: StatusBadgeProps) {
  return <span className={`badge badge-${tone}`}>{label}</span>;
}

export function formatMoney(amount: number, currency = 'CLP'): string {
  try {
    return new Intl.NumberFormat('es-CL', {
      style: 'currency',
      currency,
      maximumFractionDigits: currency === 'CLP' ? 0 : 2,
    }).format(amount);
  } catch {
    return `${amount} ${currency}`;
  }
}

export function formatDateTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat('es-CL', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date);
}
