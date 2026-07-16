import csv
import json
import sys

def main():
    input_path = sys.argv[1] if len(sys.argv) > 1 else "randomizations.json"
    output_path = input_path.replace(".json", ".csv")

    with open(input_path) as f:
        data = json.load(f)

    fieldnames = [
        "id",
        "study_id",
        "unit_kind",
        "unit_data",
        "arm_id",
        "arm_ratio",
        "arm_locator",
        "issued_at",
    ]

    with open(output_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in data:
            writer.writerow({
                "id": row["id"],
                "study_id": row["study_id"],
                "unit_kind": row["unit"]["kind"],
                "unit_data": json.dumps(row["unit"]["data"]),
                "arm_id": row["arm"]["id"],
                "arm_ratio": row["arm"]["ratio"],
                "arm_locator": row["arm"]["locator"],
                "issued_at": row["issued_at"],
            })

    print(f"Wrote {len(data)} rows to {output_path}")


if __name__ == "__main__":
    main()
