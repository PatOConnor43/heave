POST {{ baseurl }}/pet
Authorization: Bearer {{ authorization }}
Prefer: code=200
{
  "category": {
    "id": 0,
    "name": ""
  },
  "id": 0,
  "name": "",
  "photoUrls": [
    ""
  ],
  "status": "",
  "tags": [
    {
      "id": 0,
      "name": ""
    }
  ]
}
HTTP 200

[Asserts]
jsonpath "$" isCollection
jsonpath "$.id" isInteger
jsonpath "$.name" isString
jsonpath "$.category" isCollection
jsonpath "$.category.id" isInteger
jsonpath "$.category.name" isString
jsonpath "$.photoUrls" isCollection
jsonpath "$.tags" isCollection
jsonpath "$.status" isString

