import unicodedata


def slugify(text):
    ascii_text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    out, dash = [], False
    for char in ascii_text.lower():
        if char.isascii() and char.isalnum():
            out.append(char)
            dash = False
        elif not dash:
            out.append("-")
            dash = True
    return "".join(out).strip("-")


def word_wrap(text, width):
    if width < 1:
        raise ValueError("width must be positive")
    lines, current = [], ""
    for word in text.split():
        if not current:
            current = word
        elif len(current) + 1 + len(word) <= width:
            current += " " + word
        else:
            lines.append(current)
            current = word
    if current:
        lines.append(current)
    return lines
