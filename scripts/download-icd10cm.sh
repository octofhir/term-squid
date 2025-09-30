#!/usr/bin/env bash
# Download ICD-10-CM data from CDC and convert to FHIR CodeSystem format

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${SCRIPT_DIR}/../data"
ICD10_DIR="${DATA_DIR}/icd10cm"

# Create directories
mkdir -p "${ICD10_DIR}"

echo "📥 Downloading ICD-10-CM 2025 data from CDC..."
echo "Source: https://ftp.cdc.gov/pub/Health_Statistics/NCHS/Publications/ICD10CM/2026/"

# Download ICD-10-CM XML file (2026 release for services Oct 2025 - Sep 2026)
echo "Downloading ICD-10-CM tabular XML..."
curl -o "${ICD10_DIR}/icd10cm_tabular_2026.xml" \
  https://ftp.cdc.gov/pub/Health_Statistics/NCHS/Publications/ICD10CM/2026/icd10cm_tabular_2026.xml

echo "✅ Downloaded ICD-10-CM XML file"

# Create a Python converter script
cat > "${ICD10_DIR}/convert_to_fhir.py" << 'PYTHON_SCRIPT'
#!/usr/bin/env python3
"""
Convert ICD-10-CM XML data to FHIR R4 CodeSystem JSON format
"""
import xml.etree.ElementTree as ET
import json
import sys
from datetime import datetime

def convert_icd10cm_to_fhir(xml_file, output_file):
    """Convert ICD-10-CM XML to FHIR CodeSystem JSON"""

    print(f"Parsing {xml_file}...")
    tree = ET.parse(xml_file)
    root = tree.getroot()

    # Create FHIR CodeSystem structure
    code_system = {
        "resourceType": "CodeSystem",
        "id": "icd-10-cm",
        "url": "http://hl7.org/fhir/sid/icd-10-cm",
        "identifier": [{
            "system": "urn:ietf:rfc:3986",
            "value": "urn:oid:2.16.840.1.113883.6.90"
        }],
        "version": "2026",
        "name": "ICD10CM",
        "title": "International Classification of Diseases, 10th Revision, Clinical Modification (ICD-10-CM)",
        "status": "active",
        "experimental": False,
        "publisher": "Centers for Disease Control and Prevention (CDC)",
        "description": "ICD-10-CM is the official system of assigning codes to diagnoses and procedures associated with hospital utilization in the United States.",
        "copyright": "The ICD-10-CM is maintained by the Centers for Medicare & Medicaid Services (CMS) and the National Center for Health Statistics (NCHS).",
        "caseSensitive": False,
        "content": "complete",
        "concept": []
    }

    # Parse XML and extract codes
    # ICD-10-CM XML structure varies, this is a basic parser
    # Adjust based on actual XML structure
    concepts_count = 0

    # Try to find diagnosis elements (structure may vary)
    for diag in root.findall(".//diag"):
        code_elem = diag.find("name")
        desc_elem = diag.find("desc")

        if code_elem is not None and desc_elem is not None:
            code = code_elem.text.strip() if code_elem.text else ""
            display = desc_elem.text.strip() if desc_elem.text else ""

            if code and display:
                concept = {
                    "code": code,
                    "display": display
                }
                code_system["concept"].append(concept)
                concepts_count += 1

    # Alternative parsing for different XML structure
    if concepts_count == 0:
        print("Trying alternative XML structure...")
        for chapter in root.findall(".//chapter"):
            for section in chapter.findall(".//section"):
                for diag in section.findall(".//diag"):
                    name = diag.find("name")
                    desc = diag.find("desc")

                    if name is not None and desc is not None:
                        code = name.text.strip() if name.text else ""
                        display = desc.text.strip() if desc.text else ""

                        if code and display:
                            concept = {
                                "code": code,
                                "display": display
                            }
                            code_system["concept"].append(concept)
                            concepts_count += 1

    print(f"Found {concepts_count} ICD-10-CM codes")

    # Write to JSON file
    print(f"Writing FHIR CodeSystem to {output_file}...")
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump(code_system, f, indent=2, ensure_ascii=False)

    print(f"✅ Converted {concepts_count} codes to FHIR CodeSystem format")
    return concepts_count

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: convert_to_fhir.py <input_xml> <output_json>")
        sys.exit(1)

    input_file = sys.argv[1]
    output_file = sys.argv[2]

    try:
        convert_icd10cm_to_fhir(input_file, output_file)
    except Exception as e:
        print(f"❌ Error: {e}", file=sys.stderr)
        sys.exit(1)
PYTHON_SCRIPT

chmod +x "${ICD10_DIR}/convert_to_fhir.py"

echo ""
echo "🔄 Converting ICD-10-CM XML to FHIR CodeSystem JSON..."
python3 "${ICD10_DIR}/convert_to_fhir.py" \
  "${ICD10_DIR}/icd10cm_tabular_2026.xml" \
  "${ICD10_DIR}/icd10cm-codesystem.json"

echo ""
echo "✅ ICD-10-CM data downloaded and converted!"
echo "📁 FHIR CodeSystem JSON: ${ICD10_DIR}/icd10cm-codesystem.json"
echo ""
echo "To import into term-squid:"
echo "  just create-codesystem ${ICD10_DIR}/icd10cm-codesystem.json"