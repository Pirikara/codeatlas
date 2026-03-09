// Fixture for context disambiguation tests.
// Two methods named "process" in the same class — tree-sitter parses both.
export class DupProcessor {
  process(value: string): string {
    return value.trim();
  }

  process(value: number): string {
    return String(value);
  }
}
