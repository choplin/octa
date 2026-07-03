SELECT
    number     AS "number!: i64",
    title      AS "title!: String",
    body       AS "body!: String",
    state      AS "state!: String",
    created_at AS "created_at!: String",
    updated_at AS "updated_at!: String"
FROM issues
WHERE state = ?
ORDER BY number;
