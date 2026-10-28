-- R-36: the origins each item has been typed into, so the first use of a personal item at
-- a new origin can be recorded (first_use_origin). Names and origins only, never a value.
CREATE TABLE origin_uses (item_name TEXT NOT NULL, origin TEXT NOT NULL, PRIMARY KEY(item_name, origin));
