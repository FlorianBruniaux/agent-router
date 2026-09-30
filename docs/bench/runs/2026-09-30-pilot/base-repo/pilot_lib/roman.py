VALUES = {"I": 1, "V": 5, "X": 10, "L": 50, "C": 100, "D": 500, "M": 1000}


def roman_to_int(numeral):
    """Convert a Roman numeral to an integer."""
    return sum(VALUES[char] for char in numeral)
