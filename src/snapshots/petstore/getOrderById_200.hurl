GET {{ baseurl }}/store/order/{{orderId}}
Authorization: Bearer {{ authorization }}
Prefer: code=200

HTTP 200

[Asserts]
jsonpath "$" isCollection
#jsonpath "$.id" isInteger
#jsonpath "$.petId" isInteger
#jsonpath "$.quantity" isInteger
#jsonpath "$.shipDate" isString
#jsonpath "$.status" isString
#jsonpath "$.complete" isBoolean
