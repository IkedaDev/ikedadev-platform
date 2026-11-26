import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Search,
} from 'lucide-react';
import { useMemo, useState, type ReactNode } from 'react';
import { EmptyState } from './EmptyState';
import { Input } from './Input';
import { Skeleton } from './Skeleton';

export interface DataTableColumn<T> {
  key: string;
  header: string;
  render: (row: T) => ReactNode;
  mobileLabel?: string;
  align?: 'left' | 'center' | 'right';
  hideOnMobile?: boolean;
  sortable?: boolean;
  sortValue?: (row: T) => string | number | boolean | null | undefined;
}

export type SortDirection = 'asc' | 'desc';

export interface DataTableSort {
  key: string;
  direction: SortDirection;
}

export interface DataTableProps<T> {
  columns: DataTableColumn<T>[];
  data: T[];
  rowKey: (row: T) => string;
  isLoading?: boolean;
  skeletonRows?: number;
  emptyTitle?: string;
  emptyDescription?: string;
  emptyIcon?: ReactNode;
  emptyAction?: ReactNode;
  toolbar?: ReactNode;
  searchValue?: string;
  onSearchChange?: (value: string) => void;
  searchPlaceholder?: string;
  sort?: DataTableSort | null;
  onSortChange?: (next: DataTableSort | null) => void;
  page?: number;
  totalPages?: number;
  total?: number;
  onPageChange?: (page: number) => void;
}

function getVisiblePages(current: number, totalPages: number): number[] {
  const maxVisible = 5;
  if (totalPages <= maxVisible) {
    return Array.from({ length: totalPages }, (_, i) => i + 1);
  }
  let start = Math.max(1, current - 2);
  let end = Math.min(totalPages, start + maxVisible - 1);
  if (end - start + 1 < maxVisible) {
    start = Math.max(1, end - maxVisible + 1);
  }
  const pages: number[] = [];
  for (let p = start; p <= end; p += 1) {
    pages.push(p);
  }
  return pages;
}

export function DataTable<T>({
  columns,
  data,
  rowKey,
  isLoading = false,
  skeletonRows = 5,
  emptyTitle = 'No hay registros',
  emptyDescription = 'Aún no existen elementos que coincidan con tu búsqueda.',
  emptyIcon,
  emptyAction,
  toolbar,
  searchValue,
  onSearchChange,
  searchPlaceholder = 'Buscar...',
  sort: controlledSort,
  onSortChange,
  page,
  totalPages,
  total,
  onPageChange,
}: DataTableProps<T>) {
  const [internalSort, setInternalSort] = useState<DataTableSort | null>(null);

  const activeSort = controlledSort !== undefined ? controlledSort : internalSort;

  const handleToggleSort = (column: DataTableColumn<T>) => {
    if (!column.sortable) return;
    let next: DataTableSort | null;
    if (!activeSort || activeSort.key !== column.key) {
      next = { key: column.key, direction: 'asc' };
    } else if (activeSort.direction === 'asc') {
      next = { key: column.key, direction: 'desc' };
    } else {
      next = null;
    }

    if (onSortChange) {
      onSortChange(next);
    } else {
      setInternalSort(next);
    }
  };

  const displayData = useMemo(() => {
    if (!activeSort || onSortChange) {
      return data;
    }
    const col = columns.find((c) => c.key === activeSort.key);
    if (!col || !col.sortValue) return data;

    const dir = activeSort.direction === 'asc' ? 1 : -1;
    return [...data].sort((a, b) => {
      const av = col.sortValue?.(a);
      const bv = col.sortValue?.(b);
      if (av === bv) return 0;
      if (av === null || av === undefined) return 1;
      if (bv === null || bv === undefined) return -1;
      if (typeof av === 'number' && typeof bv === 'number') {
        return (av - bv) * dir;
      }
      return String(av).localeCompare(String(bv), 'es', { numeric: true }) * dir;
    });
  }, [data, activeSort, onSortChange, columns]);

  const showToolbar = Boolean(onSearchChange || toolbar);
  const showPagination =
    typeof page === 'number' &&
    typeof totalPages === 'number' &&
    typeof onPageChange === 'function' &&
    (totalPages > 1 || (typeof total === 'number' && total > 0));

  const visiblePages = showPagination && totalPages ? getVisiblePages(page!, totalPages) : [];

  return (
    <div className="datatable-shell">
      {showToolbar && (
        <div className="datatable-toolbar">
          {onSearchChange ? (
            <div className="datatable-search">
              <Input
                value={searchValue ?? ''}
                onChange={(e) => onSearchChange(e.target.value)}
                placeholder={searchPlaceholder}
                icon={<Search size={15} />}
                aria-label={searchPlaceholder}
              />
            </div>
          ) : (
            <div />
          )}
          {toolbar && <div className="datatable-toolbar-extra">{toolbar}</div>}
        </div>
      )}

      {!isLoading && displayData.length === 0 ? (
        <EmptyState
          title={emptyTitle}
          description={emptyDescription}
          icon={emptyIcon}
          action={emptyAction}
        />
      ) : (
        <div className="datatable-table-wrap">
          <table className="datatable-table">
            <thead>
              <tr>
                {columns.map((col) => {
                  const alignClass =
                    col.align === 'right'
                      ? 'datatable-align-right'
                      : col.align === 'center'
                        ? 'datatable-align-center'
                        : '';
                  const isSorted = activeSort?.key === col.key;
                  return (
                    <th
                      key={col.key}
                      className={[alignClass, col.hideOnMobile ? 'datatable-hide-mobile' : '']
                        .filter(Boolean)
                        .join(' ')}
                    >
                      {col.sortable ? (
                        <button
                          type="button"
                          className="datatable-th-btn"
                          onClick={() => handleToggleSort(col)}
                        >
                          <span>{col.header}</span>
                          {!isSorted && <ArrowUpDown size={13} />}
                          {isSorted && activeSort?.direction === 'asc' && <ArrowUp size={13} />}
                          {isSorted && activeSort?.direction === 'desc' && <ArrowDown size={13} />}
                        </button>
                      ) : (
                        col.header
                      )}
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {isLoading
                ? Array.from({ length: skeletonRows }, (_, rowIndex) => (
                    <tr key={`skeleton_${rowIndex}`}>
                      {columns.map((col) => (
                        <td
                          key={col.key}
                          data-label={col.mobileLabel ?? col.header}
                          className={col.hideOnMobile ? 'datatable-hide-mobile' : undefined}
                        >
                          <Skeleton variant="line" />
                        </td>
                      ))}
                    </tr>
                  ))
                : displayData.map((row) => (
                    <tr key={rowKey(row)}>
                      {columns.map((col) => {
                        const alignClass =
                          col.align === 'right'
                            ? 'datatable-align-right'
                            : col.align === 'center'
                              ? 'datatable-align-center'
                              : '';
                        return (
                          <td
                            key={col.key}
                            data-label={col.mobileLabel ?? col.header}
                            className={[alignClass, col.hideOnMobile ? 'datatable-hide-mobile' : '']
                              .filter(Boolean)
                              .join(' ')}
                          >
                            {col.render(row)}
                          </td>
                        );
                      })}
                    </tr>
                  ))}
            </tbody>
          </table>
        </div>
      )}

      {showPagination && (
        <div className="datatable-pagination">
          <span className="datatable-pagination-info">
            Página <strong>{page}</strong> de <strong>{Math.max(1, totalPages!)}</strong>
            {typeof total === 'number' && (
              <>
                {' '}
                · <strong>{total}</strong> registros
              </>
            )}
          </span>
          <div className="datatable-pagination-controls">
            <button
              type="button"
              className="datatable-page-btn"
              onClick={() => onPageChange!(1)}
              disabled={page! <= 1 || isLoading}
              aria-label="Primera página"
            >
              <ChevronsLeft size={14} />
            </button>
            <button
              type="button"
              className="datatable-page-btn"
              onClick={() => onPageChange!(Math.max(1, page! - 1))}
              disabled={page! <= 1 || isLoading}
              aria-label="Página anterior"
            >
              <ChevronLeft size={14} />
            </button>
            {visiblePages.map((p) => (
              <button
                key={p}
                type="button"
                className={`datatable-page-btn ${p === page ? 'active' : ''}`}
                onClick={() => onPageChange!(p)}
                disabled={isLoading}
              >
                {p}
              </button>
            ))}
            <button
              type="button"
              className="datatable-page-btn"
              onClick={() => onPageChange!(Math.min(totalPages!, page! + 1))}
              disabled={page! >= totalPages! || isLoading}
              aria-label="Página siguiente"
            >
              <ChevronRight size={14} />
            </button>
            <button
              type="button"
              className="datatable-page-btn"
              onClick={() => onPageChange!(totalPages!)}
              disabled={page! >= totalPages! || isLoading}
              aria-label="Última página"
            >
              <ChevronsRight size={14} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
