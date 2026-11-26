import {
  useId,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from 'react';

interface BaseFieldProps {
  label?: string;
  error?: string;
  hint?: string;
  icon?: ReactNode;
  required?: boolean;
}

export interface InputProps
  extends BaseFieldProps,
    Omit<InputHTMLAttributes<HTMLInputElement>, 'required'> {}

export function Input({
  label,
  error,
  hint,
  icon,
  required,
  id,
  className = '',
  ...rest
}: InputProps) {
  const autoId = useId();
  const fieldId = id ?? autoId;

  return (
    <div className="field">
      {label && (
        <label className="field-label" htmlFor={fieldId}>
          {label}
          {required && <span className="field-required">*</span>}
        </label>
      )}
      <div className="field-control">
        {icon && <span className="field-icon">{icon}</span>}
        <input
          id={fieldId}
          required={required}
          className={[
            'input',
            icon ? 'has-icon' : '',
            error ? 'is-error' : '',
            className,
          ]
            .filter(Boolean)
            .join(' ')}
          aria-invalid={Boolean(error) || undefined}
          {...rest}
        />
      </div>
      {error ? (
        <span className="field-error">{error}</span>
      ) : hint ? (
        <span className="field-hint">{hint}</span>
      ) : null}
    </div>
  );
}

export interface SelectProps
  extends BaseFieldProps,
    Omit<SelectHTMLAttributes<HTMLSelectElement>, 'required'> {
  children: ReactNode;
}

export function Select({
  label,
  error,
  hint,
  icon,
  required,
  id,
  className = '',
  children,
  ...rest
}: SelectProps) {
  const autoId = useId();
  const fieldId = id ?? autoId;

  return (
    <div className="field">
      {label && (
        <label className="field-label" htmlFor={fieldId}>
          {label}
          {required && <span className="field-required">*</span>}
        </label>
      )}
      <div className="field-control">
        {icon && <span className="field-icon">{icon}</span>}
        <select
          id={fieldId}
          required={required}
          className={[
            'select',
            icon ? 'has-icon' : '',
            error ? 'is-error' : '',
            className,
          ]
            .filter(Boolean)
            .join(' ')}
          aria-invalid={Boolean(error) || undefined}
          {...rest}
        >
          {children}
        </select>
      </div>
      {error ? (
        <span className="field-error">{error}</span>
      ) : hint ? (
        <span className="field-hint">{hint}</span>
      ) : null}
    </div>
  );
}

export interface TextareaProps
  extends Omit<BaseFieldProps, 'icon'>,
    Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, 'required'> {}

export function Textarea({
  label,
  error,
  hint,
  required,
  id,
  className = '',
  ...rest
}: TextareaProps) {
  const autoId = useId();
  const fieldId = id ?? autoId;

  return (
    <div className="field">
      {label && (
        <label className="field-label" htmlFor={fieldId}>
          {label}
          {required && <span className="field-required">*</span>}
        </label>
      )}
      <textarea
        id={fieldId}
        required={required}
        className={['textarea', error ? 'is-error' : '', className].filter(Boolean).join(' ')}
        aria-invalid={Boolean(error) || undefined}
        {...rest}
      />
      {error ? (
        <span className="field-error">{error}</span>
      ) : hint ? (
        <span className="field-hint">{hint}</span>
      ) : null}
    </div>
  );
}
