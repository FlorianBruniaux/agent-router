VALUES = {"I": 1, "V": 5, "X": 10, "L": 50, "C": 100, "D": 500, "M": 1000}


def roman_to_int(numeral):
    if not numeral or any(char not in VALUES for char in numeral):
        raise ValueError("invalid numeral")
    total = 0
    for index, char in enumerate(numeral):
        value = VALUES[char]
        if index + 1 < len(numeral) and value < VALUES[numeral[index + 1]]:
            total -= value
        else:
            total += value
    return total
