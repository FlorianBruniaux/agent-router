def parse_duration(value):
    """Parse strings such as '1h30m' or '45s' into a number of seconds."""
    units = {"h": 3600, "m": 60}
    total = 0
    number = ""
    for char in value:
        if char.isdigit():
            number += char
        else:
            total += int(number) * units[char]
            number = ""
    return total
