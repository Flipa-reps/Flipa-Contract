import React from "react";
import { LoadingSpinner } from "./LoadingSpinner";
import styles from "./Button.module.css";

export type ButtonVariant = "primary" | "secondary" | "danger";
export type ButtonSize = "sm" | "md";

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  /** Icon placed before the label. Pass an SVG element with aria-hidden="true". */
  iconStart?: React.ReactNode;
  /** Icon placed after the label. Pass an SVG element with aria-hidden="true". */
  iconEnd?: React.ReactNode;
}

export function Button({
  variant = "primary",
  size = "md",
  loading = false,
  iconStart,
  iconEnd,
  disabled,
  children,
  className,
  ...rest
}: ButtonProps) {
  const isDisabled = disabled || loading;

  return (
    <button
      {...rest}
      disabled={isDisabled}
      aria-disabled={isDisabled}
      aria-busy={loading}
      data-variant={variant}
      data-size={size}
      className={[
        styles.btn,
        styles[variant],
        styles[size],
        loading ? styles.loading : "",
        className ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {loading && (
        <LoadingSpinner
          size="small"
          label="Loading…"
          className={styles.spinner}
        />
      )}
      {!loading && iconStart && (
        <span className={styles.icon} aria-hidden="true">
          {iconStart}
        </span>
      )}
      {children && <span className={styles.label}>{children}</span>}
      {!loading && iconEnd && (
        <span className={styles.icon} aria-hidden="true">
          {iconEnd}
        </span>
      )}
    </button>
  );
}
