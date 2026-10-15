pub const REFERENCE_VIOLATIONS: &str = "
    SELECT source.kind || ' source', source.id,
           'missing ' || CASE WHEN source.item_id IS NOT NULL THEN 'item ' || source.item_id ELSE 'augment ' || source.augment_id END
    FROM sources source
    LEFT JOIN items item ON item.id = source.item_id
    LEFT JOIN augments augment ON augment.id = source.augment_id
    WHERE item.id IS NULL AND augment.id IS NULL
    UNION ALL
    SELECT vendor.name || ': ' || item.name, cost.offer_id,
           'missing ' || cost.ingredient_kind || ' ' || cost.ingredient_id
    FROM vendor_offer_costs cost JOIN vendor_offers offer ON offer.id = cost.offer_id
    JOIN vendors vendor ON vendor.id = offer.vendor_id JOIN items item ON item.id = offer.item_id
    LEFT JOIN vendor_currencies currency ON cost.ingredient_kind = 'vendor_currency' AND currency.id = cost.ingredient_id
    LEFT JOIN crafting_ingredients ingredient ON cost.ingredient_kind = 'crafting_ingredient' AND ingredient.id = cost.ingredient_id
    WHERE currency.id IS NULL AND ingredient.id IS NULL";
