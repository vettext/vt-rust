ALTER TABLE pets 
ADD COLUMN IF NOT EXISTS color TEXT,
ADD COLUMN IF NOT EXISTS species TEXT,
ADD COLUMN IF NOT EXISTS spayed_neutered BOOLEAN,
ADD COLUMN IF NOT EXISTS weight INTEGER;