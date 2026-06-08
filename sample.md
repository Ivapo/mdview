# mdview sample

A minimal terminal markdown reader with a hand-rolled renderer.

## Inline styling

Plain text, **bold**, *italic*, ***bold italic***, ~~strikethrough~~, and
`inline code` all work. Links look like [ratatui](https://ratatui.rs)
and render colored + underlined.

### A subheading

Sub-subheadings use color only — no underbar.

#### Fourth level

##### Fifth

###### Sixth

## Code blocks

```rust
fn main() {
    let greeting = "hello, mdview";
    println!("{greeting}");
    for i in 0..3 {
        println!("count = {i}");
    }
}
```

```python
def fib(n):
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a
```

```
plain text with no language tag — still gets the dark background.
```

## Lists

Unordered:

- first
- second with *emphasis*
  - nested
  - also nested
- third

Ordered:

1. one
2. two
3. three

Task list:

- [x] write renderer
- [x] handle code blocks
- [ ] world peace

## Blockquote

> A short blockquote. The left bar should be dim gray and the text
> should flow naturally inside the quoted region.

## Table

| Feature        | Status | Notes              |
|----------------|--------|--------------------|
| Headings       | ok     | h1/h2 with underbar |
| Code blocks    | ok     | syntect-highlighted |
| Tables         | ok     | box-drawing chars  |
| Footnotes      | basic  | shown as `[^name]` |

## Horizontal rule

---

That's it.
