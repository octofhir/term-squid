#!/usr/bin/env bash
# Download LOINC data and convert to FHIR CodeSystem format
# Note: LOINC requires a free account at https://loinc.org

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${SCRIPT_DIR}/../data"
LOINC_DIR="${DATA_DIR}/loinc"

# Create directories
mkdir -p "${LOINC_DIR}"

echo "📥 Downloading LOINC data..."
echo ""
echo "⚠️  LOINC requires a free account to download the full database."
echo "    Please visit: https://loinc.org/downloads/"
echo "    Download: LOINC Table File (CSV format)"
echo ""
echo "After downloading, place the Loinc.csv file in: ${LOINC_DIR}/"
echo ""

# Check if LOINC CSV exists
if [ ! -f "${LOINC_DIR}/Loinc.csv" ]; then
    echo "❌ Error: Loinc.csv not found in ${LOINC_DIR}/"
    echo ""
    echo "Steps to get LOINC:"
    echo "  1. Create free account at https://loinc.org"
    echo "  2. Download LOINC Table File (CSV)"
    echo "  3. Extract and copy Loinc.csv to ${LOINC_DIR}/"
    echo "  4. Run this script again"
    exit 1
fi

echo "✅ Found Loinc.csv"

# Alternative: Use the official LOINC FHIR CodeSystem from GitHub
echo ""
echo "📥 Downloading official LOINC FHIR CodeSystem definition from GitHub..."
curl -o "${LOINC_DIR}/loinc-codesystem-base.json" \
  https://raw.githubusercontent.com/loinc/loinc-fhir-codesystem/main/CodeSystem-loinc.json

echo "✅ Downloaded LOINC FHIR CodeSystem base definition"

# Create a Python converter script to merge CSV data with FHIR CodeSystem
cat > "${LOINC_DIR}/convert_to_fhir.py" << 'PYTHON_SCRIPT'
#!/usr/bin/env python3
"""
Convert LOINC CSV data to complete FHIR R4 CodeSystem JSON format
Merges the official LOINC FHIR CodeSystem definition with actual codes from CSV
"""
import csv
import json
import sys
from datetime import datetime

def convert_loinc_to_fhir(csv_file, base_json, output_file, limit=None):
    """Convert LOINC CSV to FHIR CodeSystem JSON"""

    print(f"Loading base FHIR CodeSystem from {base_json}...")
    with open(base_json, 'r', encoding='utf-8') as f:
        code_system = json.load(f)

    # Ensure concept array exists
    if "concept" not in code_system:
        code_system["concept"] = []

    print(f"Parsing {csv_file}...")
    concepts_count = 0

    with open(csv_file, 'r', encoding='utf-8') as f:
        reader = csv.DictReader(f)

        for row in reader:
            # LOINC CSV columns: LOINC_NUM, COMPONENT, PROPERTY, TIME_ASPCT, SYSTEM, SCALE_TYP, METHOD_TYP, CLASS, etc.
            loinc_num = row.get('LOINC_NUM', '').strip()
            long_common_name = row.get('LONG_COMMON_NAME', '').strip()
            short_name = row.get('SHORTNAME', '').strip()
            status = row.get('STATUS', 'ACTIVE').strip()

            if not loinc_num:
                continue

            # Use LONG_COMMON_NAME as display, fall back to SHORTNAME
            display = long_common_name if long_common_name else short_name

            if loinc_num and display:
                concept = {
                    "code": loinc_num,
                    "display": display
                }

                # Add status property if not active
                if status and status.upper() != 'ACTIVE':
                    concept["property"] = [{
                        "code": "status",
                        "valueCode": status
                    }]

                code_system["concept"].append(concept)
                concepts_count += 1

                # Limit for testing
                if limit and concepts_count >= limit:
                    print(f"Reached limit of {limit} concepts for testing")
                    break

            if concepts_count % 10000 == 0:
                print(f"  Processed {concepts_count} codes...")

    # Update metadata
    code_system["count"] = concepts_count
    code_system["content"] = "complete"

    print(f"Found {concepts_count} LOINC codes")

    # Write to JSON file
    print(f"Writing FHIR CodeSystem to {output_file}...")
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump(code_system, f, indent=2, ensure_ascii=False)

    print(f"✅ Converted {concepts_count} codes to FHIR CodeSystem format")
    return concepts_count

if __name__ == "__main__":
    if len(sys.argv) < 4:
        print("Usage: convert_to_fhir.py <loinc_csv> <base_json> <output_json> [limit]")
        sys.exit(1)

    csv_file = sys.argv[1]
    base_json = sys.argv[2]
    output_file = sys.argv[3]
    limit = int(sys.argv[4]) if len(sys.argv) > 4 else None

    try:
        convert_loinc_to_fhir(csv_file, base_json, output_file, limit)
    except Exception as e:
        print(f"❌ Error: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)
PYTHON_SCRIPT

chmod +x "${LOINC_DIR}/convert_to_fhir.py"

echo ""
echo "🔄 Converting LOINC CSV to FHIR CodeSystem JSON..."
echo "   (This may take a few minutes for ~90,000 codes)"

# Convert with optional limit for testing
# Remove the limit argument to process all codes
python3 "${LOINC_DIR}/convert_to_fhir.py" \
  "${LOINC_DIR}/Loinc.csv" \
  "${LOINC_DIR}/loinc-codesystem-base.json" \
  "${LOINC_DIR}/loinc-codesystem.json"

echo ""
echo "✅ LOINC data downloaded and converted!"
echo "📁 FHIR CodeSystem JSON: ${LOINC_DIR}/loinc-codesystem.json"
echo ""
echo "To import into term-squid:"
echo "  just create-codesystem ${LOINC_DIR}/loinc-codesystem.json"