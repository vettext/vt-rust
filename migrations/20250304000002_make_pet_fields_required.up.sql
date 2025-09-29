-- Make pet fields non-nullable with defaults
-- First, update existing NULL values to defaults
UPDATE pets 
SET species = 'dog' 
WHERE species IS NULL;

UPDATE pets 
SET spayed_neutered = false 
WHERE spayed_neutered IS NULL;

UPDATE pets 
SET weight = 0 
WHERE weight IS NULL;

-- Now alter the columns to be non-nullable with defaults
ALTER TABLE pets 
ALTER COLUMN species SET NOT NULL,
ALTER COLUMN species SET DEFAULT 'dog';

ALTER TABLE pets 
ALTER COLUMN spayed_neutered SET NOT NULL,
ALTER COLUMN spayed_neutered SET DEFAULT false;

ALTER TABLE pets 
ALTER COLUMN weight SET NOT NULL,
ALTER COLUMN weight SET DEFAULT 0;