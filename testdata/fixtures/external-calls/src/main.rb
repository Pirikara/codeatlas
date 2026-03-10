class DataService
  def fetch_records
    conn = ActiveRecord::Base.connection
    result = conn.execute("SELECT * FROM users")
    Marshal.load(result.to_s)
  end

  def process(data)
    puts data.inspect
    save(data)
  end

  def save(data)
    File.write("output.txt", data.to_s)
  end
end
