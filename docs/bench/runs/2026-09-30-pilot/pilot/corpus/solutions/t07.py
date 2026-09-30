import csv


def csv_sum(path, column):
    with open(path, newline="") as handle:
        rows = list(csv.reader(handle))
    if not rows or column not in rows[0]:
        raise KeyError(column)
    index = rows[0].index(column)
    total = 0.0
    for row in rows[1:]:
        if index < len(row) and row[index].strip():
            total += float(row[index])
    return total
