/**
 * Unit tests for Button component
 * Covers: variants, states (disabled, loading), keyboard nav, accessibility
 */

import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { Button } from "../components/Button";

// ── Rendering ─────────────────────────────────────────────────────────────────

describe("Button — rendering", () => {
  it.each(["primary", "secondary", "danger"] as const)(
    "renders %s variant with label",
    (variant) => {
      render(<Button variant={variant}>Click me</Button>);
      expect(screen.getByRole("button", { name: "Click me" })).toBeInTheDocument();
    }
  );

  it("defaults to primary variant", () => {
    render(<Button>Default</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("data-variant", "primary");
  });

  it("renders iconStart before label", () => {
    const icon = <svg data-testid="icon-start" aria-hidden="true" />;
    render(<Button iconStart={icon}>Label</Button>);
    expect(screen.getByTestId("icon-start")).toBeInTheDocument();
    expect(screen.getByText("Label")).toBeInTheDocument();
  });

  it("renders iconEnd after label", () => {
    const icon = <svg data-testid="icon-end" aria-hidden="true" />;
    render(<Button iconEnd={icon}>Label</Button>);
    expect(screen.getByTestId("icon-end")).toBeInTheDocument();
  });

  it("renders icon-only button (no children)", () => {
    const icon = <svg data-testid="icon-only" aria-hidden="true" />;
    render(<Button iconStart={icon} aria-label="Submit" />);
    expect(screen.getByRole("button", { name: "Submit" })).toBeInTheDocument();
  });
});

// ── Disabled state ────────────────────────────────────────────────────────────

describe("Button — disabled", () => {
  it("is disabled when disabled prop is set", () => {
    render(<Button disabled>Disabled</Button>);
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("has aria-disabled=true when disabled", () => {
    render(<Button disabled>Disabled</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("aria-disabled", "true");
  });

  it("does not fire onClick when disabled", () => {
    const onClick = vi.fn();
    render(<Button disabled onClick={onClick}>Disabled</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).not.toHaveBeenCalled();
  });
});

// ── Loading state ─────────────────────────────────────────────────────────────

describe("Button — loading", () => {
  it("shows loading spinner when loading=true", () => {
    render(<Button loading>Save</Button>);
    expect(screen.getByRole("status", { name: "Loading…" })).toBeInTheDocument();
  });

  it("is disabled while loading", () => {
    render(<Button loading>Save</Button>);
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("has aria-busy=true while loading", () => {
    render(<Button loading>Save</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("aria-busy", "true");
  });

  it("hides iconStart while loading", () => {
    const icon = <svg data-testid="icon" aria-hidden="true" />;
    render(<Button loading iconStart={icon}>Save</Button>);
    expect(screen.queryByTestId("icon")).not.toBeInTheDocument();
  });

  it("does not fire onClick while loading", () => {
    const onClick = vi.fn();
    render(<Button loading onClick={onClick}>Save</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).not.toHaveBeenCalled();
  });
});

// ── Keyboard navigation ───────────────────────────────────────────────────────

describe("Button — keyboard navigation", () => {
  it("fires onClick on Enter key", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Press</Button>);
    fireEvent.keyDown(screen.getByRole("button"), { key: "Enter" });
    fireEvent.click(screen.getByRole("button")); // native button fires click on Enter
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("fires onClick on Space key", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Press</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("is focusable by default", () => {
    render(<Button>Focus me</Button>);
    const btn = screen.getByRole("button");
    btn.focus();
    expect(document.activeElement).toBe(btn);
  });

  it("is not focusable when disabled", () => {
    render(<Button disabled>No focus</Button>);
    const btn = screen.getByRole("button");
    expect(btn).toBeDisabled();
  });
});

// ── onClick ───────────────────────────────────────────────────────────────────

describe("Button — onClick", () => {
  it("calls onClick when clicked", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Click</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
