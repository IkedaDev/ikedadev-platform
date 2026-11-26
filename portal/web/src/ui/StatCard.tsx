import type { ReactNode } from 'react';
import { Skeleton } from './Skeleton';

export interface StatCardProps {
  label: string;
  value: ReactNode;
  icon?: ReactNode;
  helper?: string;
  trend?: ReactNode;
  isLoading?: boolean;
}

export function StatCard({
  label,
  value,
  icon,
  helper,
  trend,
  isLoading = false,
}: StatCardProps) {
  if (isLoading) {
    return (
      <div className="stat-card">
        <div className="stat-card-top">
          <Skeleton variant="line-sm" />
          <Skeleton variant="circle" />
        </div>
        <Skeleton variant="line-lg" />
        <Skeleton variant="line-sm" />
      </div>
    );
  }

  return (
    <div className="stat-card">
      <div className="stat-card-top">
        <span className="stat-card-label">{label}</span>
        {icon && <span className="stat-card-icon">{icon}</span>}
      </div>
      <div className="stat-card-value">{value}</div>
      {(helper || trend) && (
        <div className="stat-card-footer">
          <span>{helper}</span>
          {trend}
        </div>
      )}
    </div>
  );
}
