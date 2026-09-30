def parse_duration(value):
    units = {"h": 3600, "m": 60, "s": 1}
    value = value.strip()
    if not value:
        raise ValueError("empty duration")
    total = 0
    number = ""
    for char in value:
        if char.isdigit():
            number += char
        elif char in units and number:
            total += int(number) * units[char]
            number = ""
        else:
            raise ValueError("invalid duration")
    if number:
        raise ValueError("missing unit")
    return total
