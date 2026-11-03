-- Native providers keep large releases out of monolithic FHIR resource JSON.
ALTER TABLE code_systems ADD COLUMN native_provider text
    CHECK (native_provider IS NULL OR native_provider = 'snomed-rf2');

CREATE TABLE snomed_releases (
    code_system_id uuid PRIMARY KEY REFERENCES code_systems(id) ON DELETE CASCADE,
    edition_module text NOT NULL,
    release_date date NOT NULL,
    dialect text NOT NULL,
    language text NOT NULL,
    fingerprint text NOT NULL CHECK (fingerprint ~ '^[0-9a-f]{64}$'),
    provenance jsonb NOT NULL
);

CREATE TABLE snomed_descriptions (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    id text NOT NULL,
    effective_time date NOT NULL,
    active boolean NOT NULL,
    module_id text NOT NULL,
    concept_code varchar(255) NOT NULL,
    language_code text NOT NULL,
    type_id text NOT NULL,
    term text NOT NULL,
    case_significance_id text NOT NULL,
    PRIMARY KEY (code_system_id, id)
);
CREATE INDEX snomed_description_concept ON snomed_descriptions(code_system_id, concept_code);

CREATE TABLE snomed_relationships (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    id text NOT NULL,
    effective_time date NOT NULL,
    active boolean NOT NULL,
    module_id text NOT NULL,
    source_code varchar(255) NOT NULL,
    destination_code varchar(255) NOT NULL,
    relationship_group integer NOT NULL CHECK (relationship_group >= 0),
    type_id text NOT NULL,
    characteristic_type_id text NOT NULL,
    modifier_id text NOT NULL,
    PRIMARY KEY (code_system_id, id)
);
CREATE INDEX snomed_inferred_parents ON snomed_relationships(code_system_id, source_code, destination_code)
    WHERE active AND type_id = '116680003' AND characteristic_type_id = '900000000000011006';

CREATE TABLE snomed_language_members (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    id uuid NOT NULL,
    effective_time date NOT NULL,
    active boolean NOT NULL,
    module_id text NOT NULL,
    refset_id text NOT NULL,
    description_id text NOT NULL,
    acceptability_id text NOT NULL,
    PRIMARY KEY (code_system_id, id),
    FOREIGN KEY (code_system_id, description_id) REFERENCES snomed_descriptions(code_system_id, id) ON DELETE CASCADE
);
CREATE INDEX snomed_language_description ON snomed_language_members(code_system_id, description_id);

CREATE TRIGGER snomed_release_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON snomed_releases
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER snomed_description_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON snomed_descriptions
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER snomed_relationship_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON snomed_relationships
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER snomed_language_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON snomed_language_members
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
