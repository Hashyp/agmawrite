# agmawrite

A deliberately minimal **Markdown editor** with *rendered preview*,
***bold italic***, `inline code`, and [links](https://iced.rs).

## Formatting

Some text with ~~strikethrough~~ and hard breaks below.

> A blockquote — quiet, like paper.

---

## Complex structures

| Feature | Editor | Preview |
|:--------|:------:|--------:|
| Headings | ✅ | ✅ |
| **Tables** | ✅ | ✅ |
| Alignment | left | right |

### Lists

- unordered item
- another item
  - nested

1. ordered item
2. second

- [ ] todo
- [x] done

### Code

```rust
fn main() -> iced::Result {
    application(boot, update, view).run()
}
```
