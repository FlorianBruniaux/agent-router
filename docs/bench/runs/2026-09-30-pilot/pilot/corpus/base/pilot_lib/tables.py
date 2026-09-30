import csv


def csv_sum(path, column):
    """Sum the numeric values of `column` in the CSV file at `path`."""
    with open(path, newline="") as handle:
        rows = list(csv.reader(handle))
    index = rows[0].index(column)
    return sum(float(row[index]) for row in rows)
