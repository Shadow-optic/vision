-- Public-record aggravators for color-of-law sentencing advocacy.
-- Counsel sets these from the record (death of the person whose rights
-- were deprived; bodily injury). They are never inferred from pending flags.
ALTER TABLE constitutional_findings
    ADD COLUMN death_resulted BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN bodily_injury BOOLEAN NOT NULL DEFAULT false;

COMMENT ON COLUMN constitutional_findings.death_resulted IS
    'True when the public record shows death resulting from the rights deprivation. Unlocks the life-imprisonment maximum under 18 U.S.C. §§ 241, 242, 1512.';
COMMENT ON COLUMN constitutional_findings.bodily_injury IS
    'True when the public record shows bodily injury. Unlocks the 10-year maximum under 18 U.S.C. § 242.';
