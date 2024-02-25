POST {{ baseurl }}/store/order
Authorization: Bearer {{ authorization }}
Prefer: code=200
{
  "complete": false,
  "id": 0,
  "petId": 0,
  "quantity": 0,
  "shipDate": "",
  "status": ""
}
HTTP 200

[Asserts]
jsonpath "$" isCollection
#jsonpath "$.id" isInteger
#jsonpath "$.petId" isInteger
#jsonpath "$.quantity" isInteger
#jsonpath "$.shipDate" isString
#jsonpath "$.status" isString
#jsonpath "$.complete" isBoolean
