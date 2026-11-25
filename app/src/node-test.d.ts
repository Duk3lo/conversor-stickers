declare module "node:test" {
  export function test(name: string, test: () => void | Promise<void>): void;
  export function describes(name: string, group: () => void): void;
}

declare module "node:assert/strict" {
  const assert: {
    equal(real: unknown, expected: unknown, message?: string): void;
    deepEqual(real: unknown, expected: unknown, message?: string): void;
    ok(value: unknown, message?: string): void;
  };
  export default assert;
}

declare module "node:fs" {
  export function readFileSync(path: URL, encoding: "utf8"): string;
}
