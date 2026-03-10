class Processor
  def process(input)
    data = parse(input)
    msg = "Result: #{data}"
    puts(msg)
    return data
  end

  def transform(obj)
    val = obj.data.name
    val
  end
end
