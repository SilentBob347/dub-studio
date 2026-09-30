import { describe, expect, it } from "vitest";
import { wordSwap } from "../src/lib/wordSwap";

describe("wordSwap", () => {
  it("finds the replaced words", () => {
    expect(wordSwap("I saw her mooney yesterday.", "I saw Hermione yesterday.")).toEqual({ from: "her mooney", to: "Hermione" });
    expect(wordSwap("Привет, Гари!", "Привет, Гарри!")).toEqual({ from: "Гари", to: "Гарри" });
  });
  it("ignores edits that are not a word swap", () => {
    expect(wordSwap("same text", "same text")).toBeNull();
    expect(wordSwap("one two", "one two three")).toBeNull();
    expect(wordSwap("a b c d e f", "g h i j k l")).toBeNull();
    expect(wordSwap("Hello, world", "hello world")).toBeNull();
  });
});
